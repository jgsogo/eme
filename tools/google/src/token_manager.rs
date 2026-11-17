//! Coordinates and caches OAuth access-token refreshes per external connection.
//!
//! Agent tools and management APIs share this manager. Concurrent calls for
//! one connection wait on the same lock and reuse the resulting access token;
//! different Google accounts refresh independently. Only the short-lived
//! access token is cached. Refresh tokens are never copied into this cache.

use std::{
    collections::HashMap,
    future::Future,
    sync::Arc,
    time::{Duration, Instant},
};

use agent_storage::ExternalConnectionId;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::{GoogleOAuthConfig, GoogleOAuthError, GoogleTokenSet, refresh_access_token};

#[derive(Default)]
pub struct GoogleOAuthTokenManager {
    connections: Mutex<HashMap<ExternalConnectionId, Arc<Mutex<Option<CachedAccessToken>>>>>,
}

struct CachedAccessToken {
    refresh_token_hash: [u8; 32],
    access_token: String,
    expires_at: Instant,
}

impl GoogleOAuthTokenManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Return a valid access token, refreshing at most once concurrently for
    /// a given connection. A changed refresh token invalidates the cached token.
    pub async fn refresh_access_token(
        &self,
        connection_id: ExternalConnectionId,
        config: &GoogleOAuthConfig,
        refresh_token: &str,
    ) -> Result<String, GoogleOAuthError> {
        let refresh_token_hash: [u8; 32] = Sha256::digest(refresh_token.as_bytes()).into();
        self.get_or_refresh(connection_id, refresh_token_hash, || {
            refresh_access_token(config, refresh_token)
        })
        .await
    }

    /// Invalidate cached access tokens after disconnect or reauthorization.
    /// The per-connection lock is retained so invalidation cannot race a refresh.
    pub async fn invalidate(&self, connection_id: ExternalConnectionId) {
        let connection = self.connections.lock().await.get(&connection_id).cloned();
        if let Some(connection) = connection {
            *connection.lock().await = None;
        }
    }

    async fn get_or_refresh<F, Fut>(
        &self,
        connection_id: ExternalConnectionId,
        refresh_token_hash: [u8; 32],
        refresh: F,
    ) -> Result<String, GoogleOAuthError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<GoogleTokenSet, GoogleOAuthError>>,
    {
        let connection = {
            let mut connections = self.connections.lock().await;
            connections
                .entry(connection_id)
                .or_insert_with(|| Arc::new(Mutex::new(None)))
                .clone()
        };
        let mut cached = connection.lock().await;
        if let Some(token) = cached.as_ref().filter(|token| {
            token.refresh_token_hash == refresh_token_hash
                && token.expires_at > Instant::now() + Duration::from_secs(30)
        }) {
            return Ok(token.access_token.clone());
        }

        let token = refresh().await?;
        let access_token = token.access_token;
        // Google normally returns a one-hour token. Use a conservative fallback
        // when the provider omits expires_in, stop using it 30 seconds early,
        // and cap local reuse at one minute to limit stale-token time after revocation.
        let lifetime = token.expires_in_seconds.unwrap_or(300).saturating_sub(30).clamp(1, 60);
        *cached = Some(CachedAccessToken {
            refresh_token_hash,
            access_token: access_token.clone(),
            expires_at: Instant::now() + Duration::from_secs(lifetime),
        });
        Ok(access_token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::time::sleep;

    fn token(access_token: &str) -> GoogleTokenSet {
        GoogleTokenSet {
            access_token: access_token.into(),
            refresh_token: None,
            expires_in_seconds: Some(3600),
            scopes: Vec::new(),
        }
    }

    #[tokio::test]
    async fn concurrent_refreshes_for_one_connection_share_one_access_token() {
        let manager = Arc::new(GoogleOAuthTokenManager::new());
        let connection_id = ExternalConnectionId::new();
        let refresh_count = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();

        for _ in 0..8 {
            let manager = manager.clone();
            let refresh_count = refresh_count.clone();
            tasks.push(tokio::spawn(async move {
                manager
                    .get_or_refresh(connection_id, [7; 32], || async move {
                        refresh_count.fetch_add(1, Ordering::SeqCst);
                        sleep(Duration::from_millis(10)).await;
                        Ok(token("shared-access-token"))
                    })
                    .await
                    .unwrap()
            }));
        }

        for task in tasks {
            assert_eq!(task.await.unwrap(), "shared-access-token");
        }
        assert_eq!(refresh_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn different_connections_can_refresh_in_parallel() {
        let manager = Arc::new(GoogleOAuthTokenManager::new());
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();

        for _ in 0..2 {
            let manager = manager.clone();
            let active = active.clone();
            let maximum = maximum.clone();
            tasks.push(tokio::spawn(async move {
                manager
                    .get_or_refresh(ExternalConnectionId::new(), [8; 32], || async move {
                        let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                        maximum.fetch_max(current, Ordering::SeqCst);
                        sleep(Duration::from_millis(30)).await;
                        active.fetch_sub(1, Ordering::SeqCst);
                        Ok(token("independent-access-token"))
                    })
                    .await
                    .unwrap();
            }));
        }

        for task in tasks {
            task.await.unwrap();
        }
        assert_eq!(maximum.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn explicit_invalidation_forces_a_new_refresh() {
        let manager = GoogleOAuthTokenManager::new();
        let connection_id = ExternalConnectionId::new();
        let first = manager
            .get_or_refresh(connection_id, [3; 32], || async { Ok(token("first")) })
            .await
            .unwrap();
        manager.invalidate(connection_id).await;
        let second = manager
            .get_or_refresh(connection_id, [3; 32], || async { Ok(token("second")) })
            .await
            .unwrap();
        assert_eq!(first, "first");
        assert_eq!(second, "second");
    }

    #[tokio::test]
    async fn a_changed_refresh_token_invalidates_the_cached_access_token() {
        let manager = GoogleOAuthTokenManager::new();
        let connection_id = ExternalConnectionId::new();
        let first = manager
            .get_or_refresh(connection_id, [1; 32], || async { Ok(token("first")) })
            .await
            .unwrap();
        let second = manager
            .get_or_refresh(connection_id, [2; 32], || async { Ok(token("second")) })
            .await
            .unwrap();
        assert_eq!(first, "first");
        assert_eq!(second, "second");
    }
}
