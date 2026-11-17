//! Serializes OAuth refreshes per external connection.
//!
//! Every Google Calendar entry point (agent tools and management APIs) shares
//! this manager. A connection gets its own async mutex, so concurrent requests
//! for one account cannot race through the provider token endpoint while
//! unrelated Google accounts remain independent.

use std::{collections::HashMap, future::Future, sync::Arc};

use agent_storage::ExternalConnectionId;
use tokio::sync::Mutex;

use crate::{GoogleOAuthConfig, GoogleOAuthError, GoogleTokenSet, refresh_access_token};

#[derive(Default)]
pub struct GoogleOAuthTokenManager {
    locks: Mutex<HashMap<ExternalConnectionId, Arc<Mutex<()>>>>,
}

impl GoogleOAuthTokenManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Refresh an access token while holding the lock for this connection.
    /// The lock is deliberately held across the network request: a second
    /// invocation for this same connection waits instead of racing it.
    pub async fn refresh_access_token(
        &self,
        connection_id: ExternalConnectionId,
        config: &GoogleOAuthConfig,
        refresh_token: &str,
    ) -> Result<GoogleTokenSet, GoogleOAuthError> {
        self.run_serialized(connection_id, || refresh_access_token(config, refresh_token))
            .await
    }

    async fn run_serialized<T, F, Fut>(&self, connection_id: ExternalConnectionId, operation: F) -> T
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        let lock = {
            let mut locks = self.locks.lock().await;
            locks
                .entry(connection_id)
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        let _guard = lock.lock().await;
        operation().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::time::{Duration, sleep};

    #[tokio::test]
    async fn concurrent_refreshes_for_one_connection_are_serialized() {
        let manager = Arc::new(GoogleOAuthTokenManager::new());
        let connection_id = ExternalConnectionId::new();
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();

        for _ in 0..8 {
            let manager = manager.clone();
            let active = active.clone();
            let maximum = maximum.clone();
            tasks.push(tokio::spawn(async move {
                manager
                    .run_serialized(connection_id, || async move {
                        let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                        maximum.fetch_max(current, Ordering::SeqCst);
                        sleep(Duration::from_millis(10)).await;
                        active.fetch_sub(1, Ordering::SeqCst);
                    })
                    .await;
            }));
        }

        for task in tasks {
            task.await.unwrap();
        }
        assert_eq!(maximum.load(Ordering::SeqCst), 1);
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
                    .run_serialized(ExternalConnectionId::new(), || async move {
                        let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                        maximum.fetch_max(current, Ordering::SeqCst);
                        sleep(Duration::from_millis(30)).await;
                        active.fetch_sub(1, Ordering::SeqCst);
                    })
                    .await;
            }));
        }

        for task in tasks {
            task.await.unwrap();
        }
        assert_eq!(maximum.load(Ordering::SeqCst), 2);
    }
}
