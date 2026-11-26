//! Trakt OAuth account linking and refresh-token lifecycle.
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use agent_storage::{
    ExternalConnectionId, ExternalConnectionStatus, ExternalConnectionStore, FamilyId, FamilyRole,
    FamilyStore, NewExternalAuthorization, NewExternalConnection, UserId,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use url::Url;
use uuid::Uuid;

use crate::{
    MediaExternalConnectionLink, MediaProfileId, MediaStore, MediaStoreError, PendingTraktLink,
};

const DEFAULT_AUTHORIZE_URL: &str = "https://auth.trakt.tv/oauth/authorize";
const DEFAULT_TOKEN_URL: &str = "https://api.trakt.tv/oauth/token";
const DEFAULT_API_BASE_URL: &str = "https://api.trakt.tv";

/// OAuth application credentials and Trakt API endpoints.
#[derive(Clone)]
pub struct TraktOAuthConfig {
    pub client_id: String,
    pub redirect_uri: String,
    pub authorize_url: String,
    pub token_url: String,
    pub api_base_url: String,
}

impl TraktOAuthConfig {
    pub fn new(
        client_id: impl Into<String>,
        redirect_uri: impl Into<String>,
    ) -> Result<Self, TraktError> {
        Self::with_endpoints(
            client_id,
            redirect_uri,
            DEFAULT_AUTHORIZE_URL,
            DEFAULT_TOKEN_URL,
            DEFAULT_API_BASE_URL,
        )
    }

    /// Custom endpoints allow deterministic OAuth tests without contacting Trakt.
    pub fn with_endpoints(
        client_id: impl Into<String>,
        redirect_uri: impl Into<String>,
        authorize_url: impl Into<String>,
        token_url: impl Into<String>,
        api_base_url: impl Into<String>,
    ) -> Result<Self, TraktError> {
        let config = Self {
            client_id: client_id.into(),
            redirect_uri: redirect_uri.into(),
            authorize_url: authorize_url.into(),
            token_url: token_url.into(),
            api_base_url: api_base_url.into().trim_end_matches('/').to_owned(),
        };
        if config.client_id.trim().is_empty() || config.redirect_uri.trim().is_empty() {
            return Err(TraktError::Configuration(
                "client ID and redirect URI are required".into(),
            ));
        }
        Url::parse(&config.redirect_uri)
            .map_err(|error| TraktError::Configuration(error.to_string()))?;
        Url::parse(&config.authorize_url)
            .map_err(|error| TraktError::Configuration(error.to_string()))?;
        Url::parse(&config.token_url)
            .map_err(|error| TraktError::Configuration(error.to_string()))?;
        Url::parse(&config.api_base_url)
            .map_err(|error| TraktError::Configuration(error.to_string()))?;
        Ok(config)
    }
}

/// URL and expiry for a pending Trakt consent flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraktAuthorization {
    pub authorization_url: String,
    pub expires_at: DateTime<Utc>,
}

/// A Trakt OAuth account-linking and credential service.
pub struct TraktOAuthService {
    config: TraktOAuthConfig,
    http: Client,
    media: MediaStore,
    families: Arc<dyn FamilyStore>,
    external_connections: Arc<dyn ExternalConnectionStore>,
    token_manager: TraktTokenManager,
}

impl TraktOAuthService {
    pub fn new(
        config: TraktOAuthConfig,
        media: MediaStore,
        families: Arc<dyn FamilyStore>,
        external_connections: Arc<dyn ExternalConnectionStore>,
    ) -> Result<Self, TraktError> {
        let http = http_client()?;
        let token_manager = TraktTokenManager::new(config.clone(), external_connections.clone())?;
        Ok(Self {
            config,
            http,
            media,
            families,
            external_connections,
            token_manager,
        })
    }

    /// Begin linking a Trakt account to a media profile.
    ///
    /// Only the EME user assigned to the media profile can attach their own
    /// external account. The pending state is hashed and consumed once.
    pub async fn begin_link(
        &self,
        owner_id: UserId,
        family_id: FamilyId,
        profile_id: MediaProfileId,
    ) -> Result<TraktAuthorization, TraktError> {
        self.require_profile_owner(owner_id, family_id, profile_id)
            .await?;
        let state = Uuid::new_v4().to_string();
        let (pkce_verifier, code_challenge) = pkce_pair();
        let expires_at = Utc::now() + ChronoDuration::minutes(10);
        self.media
            .create_pending_trakt_link(
                &state,
                &PendingTraktLink {
                    owner_id,
                    family_id: Some(family_id),
                    profile_id,
                },
                expires_at,
            )
            .await
            .map_err(TraktError::MediaStorage)?;
        self.external_connections
            .begin_authorization(NewExternalAuthorization {
                user_id: owner_id,
                provider: "trakt".into(),
                state: state.clone(),
                pkce_verifier,
                requested_scopes: Vec::new(),
                expires_at,
            })
            .await
            .map_err(TraktError::ExternalStorage)?;

        let mut url = Url::parse(&self.config.authorize_url)
            .map_err(|error| TraktError::Configuration(error.to_string()))?;
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", &self.config.client_id)
            .append_pair("redirect_uri", &self.config.redirect_uri)
            .append_pair("code_challenge", &code_challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &state);
        Ok(TraktAuthorization {
            authorization_url: url.into(),
            expires_at,
        })
    }

    /// Start OAuth for a personal profile without requiring a family membership.
    pub async fn begin_personal_link(
        &self,
        owner_id: UserId,
        profile_id: MediaProfileId,
    ) -> Result<TraktAuthorization, TraktError> {
        self.require_personal_profile_owner(owner_id, profile_id)
            .await?;
        let state = Uuid::new_v4().to_string();
        let (pkce_verifier, code_challenge) = pkce_pair();
        let expires_at = Utc::now() + ChronoDuration::minutes(10);
        self.media
            .create_pending_trakt_link(
                &state,
                &PendingTraktLink {
                    owner_id,
                    family_id: None,
                    profile_id,
                },
                expires_at,
            )
            .await
            .map_err(TraktError::MediaStorage)?;
        self.external_connections
            .begin_authorization(NewExternalAuthorization {
                user_id: owner_id,
                provider: "trakt".into(),
                state: state.clone(),
                pkce_verifier,
                requested_scopes: Vec::new(),
                expires_at,
            })
            .await
            .map_err(TraktError::ExternalStorage)?;
        let mut url = Url::parse(&self.config.authorize_url)
            .map_err(|error| TraktError::Configuration(error.to_string()))?;
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", &self.config.client_id)
            .append_pair("redirect_uri", &self.config.redirect_uri)
            .append_pair("code_challenge", &code_challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &state);
        Ok(TraktAuthorization {
            authorization_url: url.into(),
            expires_at,
        })
    }

    /// Complete a callback, validate its one-time state, and persist the encrypted refresh token.
    pub async fn complete_link(
        &self,
        expected_owner_id: UserId,
        state: &str,
        code: &str,
    ) -> Result<MediaExternalConnectionLink, TraktError> {
        if code.trim().is_empty() {
            return Err(TraktError::InvalidResponse(
                "authorization code must not be empty".into(),
            ));
        }
        let pending = self
            .media
            .get_pending_trakt_link(state)
            .await
            .map_err(TraktError::MediaStorage)?;
        if pending.owner_id != expected_owner_id {
            return Err(TraktError::Forbidden);
        }
        let pending = self
            .media
            .consume_pending_trakt_link(state, Utc::now())
            .await
            .map_err(TraktError::MediaStorage)?;
        let authorization = self
            .external_connections
            .consume_authorization("trakt", state, Utc::now())
            .await
            .map_err(TraktError::ExternalStorage)?;
        if authorization.user_id != pending.owner_id {
            return Err(TraktError::Forbidden);
        }
        if let Some(family_id) = pending.family_id {
            self.require_profile_owner(pending.owner_id, family_id, pending.profile_id)
                .await?;
        } else {
            self.require_personal_profile_owner(pending.owner_id, pending.profile_id)
                .await?;
        }

        let tokens = self
            .exchange_code(code, &authorization.pkce_verifier)
            .await?;
        let refresh_token = tokens
            .refresh_token
            .clone()
            .filter(|token| !token.is_empty())
            .ok_or(TraktError::MissingRefreshToken)?;
        let user = self.fetch_user_settings(&tokens.access_token).await?;
        let granted_scopes = if tokens.scopes().is_empty() {
            authorization.requested_scopes
        } else {
            tokens.scopes()
        };
        let existing = self
            .external_connections
            .list_connections(pending.owner_id)
            .await
            .map_err(TraktError::ExternalStorage)?
            .into_iter()
            .find(|connection| {
                connection.provider == "trakt" && connection.provider_subject == user.username
            });
        let connection = if let Some(existing) = existing {
            self.external_connections
                .update_credentials(
                    pending.owner_id,
                    existing.id,
                    Some(refresh_token),
                    granted_scopes,
                )
                .await
                .map_err(TraktError::ExternalStorage)?
        } else {
            self.external_connections
                .create_connection(NewExternalConnection {
                    owner_id: pending.owner_id,
                    provider: "trakt".into(),
                    provider_subject: user.username.clone(),
                    display_email: None,
                    refresh_token,
                    granted_scopes,
                })
                .await
                .map_err(TraktError::ExternalStorage)?
        };

        let previous_link = self
            .media
            .get_external_connection_link(pending.profile_id, "trakt")
            .await
            .map_err(TraktError::MediaStorage)?;
        let link = MediaExternalConnectionLink {
            owner_id: pending.owner_id,
            family_id: pending.family_id,
            profile_id: pending.profile_id,
            provider: "trakt".into(),
            external_connection_id: connection.id,
            provider_subject: user.username,
            display_name: user.name,
            linked_at: Utc::now(),
        };
        self.media
            .save_external_connection_link(&link)
            .await
            .map_err(TraktError::MediaStorage)?;
        if let Some(previous) = previous_link
            && previous.external_connection_id != link.external_connection_id
        {
            self.external_connections
                .revoke_connection(previous.owner_id, previous.external_connection_id)
                .await
                .map_err(TraktError::ExternalStorage)?;
            self.token_manager
                .invalidate(previous.external_connection_id)
                .await;
        }
        Ok(link)
    }

    /// Remove a profile link and revoke its encrypted external credential.
    pub async fn unlink_profile(
        &self,
        actor_id: UserId,
        family_id: FamilyId,
        profile_id: MediaProfileId,
    ) -> Result<(), TraktError> {
        if self
            .media
            .get_profile(family_id, profile_id)
            .await
            .map_err(TraktError::MediaStorage)?
            .is_none()
        {
            return Err(TraktError::Forbidden);
        }
        let link = self
            .media
            .get_external_connection_link(profile_id, "trakt")
            .await
            .map_err(TraktError::MediaStorage)?
            .ok_or(TraktError::ConnectionUnavailable)?;
        if link.owner_id != actor_id && !self.is_family_admin(actor_id, family_id).await? {
            return Err(TraktError::Forbidden);
        }
        self.revoke_connection_if_present(link.owner_id, link.external_connection_id)
            .await?;
        self.media
            .delete_external_connection_link(profile_id, "trakt")
            .await
            .map_err(TraktError::MediaStorage)?;
        self.token_manager
            .invalidate(link.external_connection_id)
            .await;
        Ok(())
    }

    /// Revoke credentials when they still exist and are not already revoked.
    ///
    /// Broken Trakt links may point to a revoked or missing external connection.
    /// Those states must not prevent removal of the profile-level link.
    async fn revoke_connection_if_present(
        &self,
        owner_id: UserId,
        connection_id: ExternalConnectionId,
    ) -> Result<(), TraktError> {
        let connection = self
            .external_connections
            .get_connection(owner_id, connection_id)
            .await
            .map_err(TraktError::ExternalStorage)?;
        if let Some(connection) = connection
            && connection.status != ExternalConnectionStatus::Revoked
        {
            self.external_connections
                .revoke_connection(owner_id, connection_id)
                .await
                .map_err(TraktError::ExternalStorage)?;
        }
        Ok(())
    }

    async fn is_family_admin(
        &self,
        user_id: UserId,
        family_id: FamilyId,
    ) -> Result<bool, TraktError> {
        let families = self
            .families
            .list_families(user_id)
            .await
            .map_err(|error| TraktError::Family(error.to_string()))?;
        if !families.iter().any(|family| family.id == family_id) {
            return Ok(false);
        }
        let members = self
            .families
            .list_members(family_id)
            .await
            .map_err(|error| TraktError::Family(error.to_string()))?;
        Ok(members.iter().any(|member| {
            member.user_id == user_id
                && matches!(member.role, FamilyRole::Owner | FamilyRole::Admin)
        }))
    }

    /// Validate that the actor still owns the assigned profile and belongs to its family.
    pub async fn validate_profile_owner(
        &self,
        user_id: UserId,
        family_id: FamilyId,
        profile_id: MediaProfileId,
    ) -> Result<(), TraktError> {
        let profile = self
            .media
            .get_profile(family_id, profile_id)
            .await
            .map_err(TraktError::MediaStorage)?
            .ok_or(TraktError::Forbidden)?;
        if profile.user_id != Some(user_id) {
            return Err(TraktError::Forbidden);
        }
        let families = self
            .families
            .list_families(user_id)
            .await
            .map_err(|error| TraktError::Family(error.to_string()))?;
        if !families.iter().any(|family| family.id == family_id) {
            return Err(TraktError::Forbidden);
        }
        let members = self
            .families
            .list_members(family_id)
            .await
            .map_err(|error| TraktError::Family(error.to_string()))?;
        if members.iter().any(|member| member.user_id == user_id) {
            Ok(())
        } else {
            Err(TraktError::Forbidden)
        }
    }

    /// Validate direct ownership of a personal profile.
    pub async fn validate_personal_profile_owner(
        &self,
        user_id: UserId,
        profile_id: MediaProfileId,
    ) -> Result<(), TraktError> {
        self.require_personal_profile_owner(user_id, profile_id)
            .await
    }

    async fn require_personal_profile_owner(
        &self,
        owner_id: UserId,
        profile_id: MediaProfileId,
    ) -> Result<(), TraktError> {
        self.media
            .get_personal_profile(owner_id, profile_id)
            .await
            .map_err(TraktError::MediaStorage)?
            .ok_or(TraktError::Forbidden)?;
        Ok(())
    }

    /// Verify that a linked profile can still authenticate with Trakt.
    ///
    /// This performs a read-only request to Trakt's account settings endpoint.
    pub async fn test_connection(
        &self,
        owner_id: UserId,
        family_id: Option<FamilyId>,
        profile_id: MediaProfileId,
    ) -> Result<String, TraktError> {
        match family_id {
            Some(family_id) => {
                self.validate_profile_owner(owner_id, family_id, profile_id)
                    .await?;
            }
            None => {
                self.validate_personal_profile_owner(owner_id, profile_id)
                    .await?;
            }
        }
        let link = self
            .media
            .get_external_connection_link(profile_id, "trakt")
            .await
            .map_err(TraktError::MediaStorage)?
            .ok_or(TraktError::ConnectionUnavailable)?;
        if link.owner_id != owner_id {
            return Err(TraktError::Forbidden);
        }
        let access_token = match self
            .token_manager
            .access_token(owner_id, link.external_connection_id)
            .await
        {
            Ok(token) => token,
            Err(error) => {
                let permanent = matches!(&error, TraktError::MissingRefreshToken)
                    || matches!(&error, TraktError::Api { status, message }
                        if (*status == StatusCode::UNAUTHORIZED
                            || *status == StatusCode::FORBIDDEN
                            || (*status == StatusCode::BAD_REQUEST
                                && message.to_ascii_lowercase().contains("invalid_grant"))));
                if permanent {
                    let _ = self
                        .external_connections
                        .mark_connection_broken(owner_id, link.external_connection_id)
                        .await;
                }
                return Err(error);
            }
        };
        let user = match self.fetch_user_settings(&access_token).await {
            Ok(user) => user,
            Err(error) => {
                let permanent = matches!(&error, TraktError::Api { status, .. }
                    if *status == StatusCode::UNAUTHORIZED);
                if permanent {
                    let _ = self
                        .external_connections
                        .mark_connection_broken(owner_id, link.external_connection_id)
                        .await;
                }
                return Err(error);
            }
        };
        if user.username != link.provider_subject {
            let _ = self
                .external_connections
                .mark_connection_broken(owner_id, link.external_connection_id)
                .await;
            return Err(TraktError::InvalidResponse(
                "Trakt returned a different account than the one linked to this profile".into(),
            ));
        }
        Ok(user.username)
    }

    /// Return the persisted status of a linked Trakt account.
    pub async fn connection_status(
        &self,
        owner_id: UserId,
        profile_id: MediaProfileId,
    ) -> Result<Option<ExternalConnectionStatus>, TraktError> {
        let Some(link) = self
            .media
            .get_external_connection_link(profile_id, "trakt")
            .await
            .map_err(TraktError::MediaStorage)?
        else {
            return Ok(None);
        };
        if link.owner_id != owner_id {
            return Err(TraktError::Forbidden);
        }
        let connection = self
            .external_connections
            .get_connection(owner_id, link.external_connection_id)
            .await
            .map_err(TraktError::ExternalStorage)?;
        // A linked profile whose external credential is revoked or no longer
        // exists cannot recover through a health check; it must be relinked.
        Ok(Some(match connection.map(|connection| connection.status) {
            Some(ExternalConnectionStatus::Active) => ExternalConnectionStatus::Active,
            Some(ExternalConnectionStatus::Broken | ExternalConnectionStatus::Revoked) | None => {
                ExternalConnectionStatus::Broken
            }
        }))
    }

    /// Disconnect a Trakt account from the caller's own personal profile.
    pub async fn unlink_personal_profile(
        &self,
        actor_id: UserId,
        profile_id: MediaProfileId,
    ) -> Result<(), TraktError> {
        self.require_personal_profile_owner(actor_id, profile_id)
            .await?;
        let link = self
            .media
            .get_external_connection_link(profile_id, "trakt")
            .await
            .map_err(TraktError::MediaStorage)?
            .ok_or(TraktError::ConnectionUnavailable)?;
        if link.owner_id != actor_id {
            return Err(TraktError::Forbidden);
        }
        self.revoke_connection_if_present(link.owner_id, link.external_connection_id)
            .await?;
        self.media
            .delete_external_connection_link(profile_id, "trakt")
            .await
            .map_err(TraktError::MediaStorage)?;
        self.token_manager
            .invalidate(link.external_connection_id)
            .await;
        Ok(())
    }

    /// Access-token manager used by Trakt history and rating synchronization.
    pub fn token_manager(&self) -> &TraktTokenManager {
        &self.token_manager
    }

    async fn require_profile_owner(
        &self,
        owner_id: UserId,
        family_id: FamilyId,
        profile_id: MediaProfileId,
    ) -> Result<(), TraktError> {
        let profile = self
            .media
            .get_profile(family_id, profile_id)
            .await
            .map_err(TraktError::MediaStorage)?
            .ok_or(TraktError::Forbidden)?;
        if profile.user_id != Some(owner_id) {
            return Err(TraktError::Forbidden);
        }
        let families = self
            .families
            .list_families(owner_id)
            .await
            .map_err(|error| TraktError::Family(error.to_string()))?;
        if !families.iter().any(|family| family.id == family_id) {
            return Err(TraktError::Forbidden);
        }
        Ok(())
    }

    async fn exchange_code(
        &self,
        code: &str,
        code_verifier: &str,
    ) -> Result<TraktTokenResponse, TraktError> {
        self.token_request(&[
            ("code", code),
            ("client_id", &self.config.client_id),
            ("redirect_uri", &self.config.redirect_uri),
            ("grant_type", "authorization_code"),
            ("code_verifier", code_verifier),
        ])
        .await
    }

    async fn fetch_user_settings(&self, access_token: &str) -> Result<TraktUser, TraktError> {
        let response = self
            .http
            .get(format!("{}/users/settings", self.config.api_base_url))
            .bearer_auth(access_token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header("trakt-api-version", "2")
            .header("trakt-api-key", &self.config.client_id)
            .send()
            .await?;
        let response = ensure_success(response).await?;
        let settings: TraktUserSettings = response.json().await?;
        if settings.user.username.trim().is_empty() {
            return Err(TraktError::InvalidResponse(
                "Trakt account did not return a username".into(),
            ));
        }
        Ok(settings.user)
    }

    async fn token_request(
        &self,
        fields: &[(&str, &str)],
    ) -> Result<TraktTokenResponse, TraktError> {
        let response = self
            .http
            .post(&self.config.token_url)
            .json(&TokenRequest(fields))
            .send()
            .await?;
        ensure_success(response)
            .await?
            .json()
            .await
            .map_err(TraktError::Request)
    }
}

/// Cached short-lived access tokens; refresh tokens remain in agent-storage only.
#[derive(Clone)]
pub struct TraktTokenManager {
    config: TraktOAuthConfig,
    http: Client,
    external_connections: Arc<dyn ExternalConnectionStore>,
    connections: Arc<Mutex<HashMap<ExternalConnectionId, Arc<Mutex<Option<CachedAccessToken>>>>>>,
}

struct CachedAccessToken {
    refresh_token_hash: [u8; 32],
    access_token: String,
    expires_at: Instant,
}

impl TraktTokenManager {
    pub fn new(
        config: TraktOAuthConfig,
        external_connections: Arc<dyn ExternalConnectionStore>,
    ) -> Result<Self, TraktError> {
        Ok(Self {
            config,
            http: http_client()?,
            external_connections,
            connections: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Return a valid token, refreshing once under concurrent requests per connection.
    pub async fn access_token(
        &self,
        owner_id: UserId,
        connection_id: ExternalConnectionId,
    ) -> Result<String, TraktError> {
        let connection_metadata = self
            .external_connections
            .get_connection(owner_id, connection_id)
            .await
            .map_err(TraktError::ExternalStorage)?
            .ok_or(TraktError::ConnectionUnavailable)?;
        if connection_metadata.status != ExternalConnectionStatus::Active {
            return Err(TraktError::ConnectionUnavailable);
        }
        let refresh_token = self
            .external_connections
            .refresh_token(owner_id, connection_id)
            .await
            .map_err(TraktError::ExternalStorage)?;
        let refresh_token_hash: [u8; 32] = Sha256::digest(refresh_token.as_bytes()).into();
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

        let token = self.refresh(&refresh_token).await?;
        let new_refresh_token = token
            .refresh_token
            .clone()
            .ok_or(TraktError::MissingRefreshToken)?;
        let scopes = if token.scope.is_some() {
            token.scopes()
        } else {
            connection_metadata.granted_scopes.clone()
        };
        self.external_connections
            .update_credentials(
                owner_id,
                connection_id,
                Some(new_refresh_token.clone()),
                scopes,
            )
            .await
            .map_err(TraktError::ExternalStorage)?;
        let lifetime = token
            .expires_in_seconds()
            .unwrap_or(300)
            .saturating_sub(30)
            .max(1);
        let access_token = token.access_token;
        let effective_refresh_token_hash: [u8; 32] =
            Sha256::digest(new_refresh_token.as_bytes()).into();
        *cached = Some(CachedAccessToken {
            refresh_token_hash: effective_refresh_token_hash,
            access_token: access_token.clone(),
            expires_at: Instant::now() + Duration::from_secs(lifetime),
        });
        Ok(access_token)
    }

    pub async fn invalidate(&self, connection_id: ExternalConnectionId) {
        let connection = self.connections.lock().await.get(&connection_id).cloned();
        if let Some(connection) = connection {
            *connection.lock().await = None;
        }
    }

    async fn refresh(&self, refresh_token: &str) -> Result<TraktTokenResponse, TraktError> {
        let response = self
            .http
            .post(&self.config.token_url)
            .json(&TokenRequest(&[
                ("refresh_token", refresh_token),
                ("client_id", &self.config.client_id),
                ("redirect_uri", &self.config.redirect_uri),
                ("grant_type", "refresh_token"),
            ]))
            .send()
            .await?;
        ensure_success(response)
            .await?
            .json()
            .await
            .map_err(TraktError::Request)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TraktError {
    #[error("invalid Trakt OAuth configuration: {0}")]
    Configuration(String),
    #[error("Trakt request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Trakt returned HTTP {status}: {message}")]
    Api { status: StatusCode, message: String },
    #[error("Trakt account did not issue a refresh token")]
    MissingRefreshToken,
    #[error("Trakt external connection is missing or revoked")]
    ConnectionUnavailable,
    #[error("family membership or profile permissions do not allow this operation")]
    Forbidden,
    #[error("family storage failed: {0}")]
    Family(String),
    #[error("media storage failed: {0}")]
    MediaStorage(#[source] MediaStoreError),
    #[error("external connection storage failed: {0}")]
    ExternalStorage(#[source] agent_storage::ExternalConnectionStorageError),
    #[error("Trakt returned an invalid response: {0}")]
    InvalidResponse(String),
}

fn pkce_pair() -> (String, String) {
    // Two UUIDs provide 256 bits of entropy and a verifier within RFC 7636's
    // 43-128 character range. Only the encrypted verifier is persisted.
    let verifier = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

fn http_client() -> Result<Client, TraktError> {
    Client::builder()
        .user_agent(concat!("EME/", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(TraktError::Request)
}

async fn ensure_success(response: reqwest::Response) -> Result<reqwest::Response, TraktError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let message = response
        .text()
        .await
        .unwrap_or_else(|_| "request failed".into());
    Err(TraktError::Api { status, message })
}

struct TokenRequest<'a>(&'a [(&'a str, &'a str)]);

impl Serialize for TokenRequest<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in self.0 {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

#[derive(Debug, Deserialize)]
struct TraktTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    scope: Option<String>,
}

impl TraktTokenResponse {
    fn scopes(&self) -> Vec<String> {
        self.scope
            .as_deref()
            .unwrap_or_default()
            .split([',', ' '])
            .filter(|scope| !scope.is_empty())
            .map(str::to_owned)
            .collect()
    }

    fn expires_in_seconds(&self) -> Option<u64> {
        self.expires_in
    }
}

#[derive(Debug, Deserialize)]
struct TraktUserSettings {
    user: TraktUser,
}

#[derive(Debug, Clone, Deserialize)]
struct TraktUser {
    username: String,
    name: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::Arc};

    use agent_storage::{
        Family, FamilyMember, FamilyRole, InMemoryFamilyStore, SqliteExternalConnectionStore,
    };
    use chrono::Utc;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path},
    };

    use super::*;

    async fn family_and_profile() -> (
        Arc<InMemoryFamilyStore>,
        UserId,
        FamilyId,
        MediaStore,
        MediaProfileId,
    ) {
        let families = Arc::new(InMemoryFamilyStore::default());
        let user_id = UserId::new();
        let family_id = FamilyId::new();
        let now = Utc::now();
        families
            .create_family(
                Family {
                    id: family_id,
                    name: "Home".into(),
                    creator_id: user_id,
                    created_at: now,
                    updated_at: now,
                },
                FamilyMember {
                    family_id,
                    user_id,
                    role: FamilyRole::Owner,
                    relationship: agent_storage::FamilyRelationship::Father,
                    joined_at: now,
                },
            )
            .await
            .unwrap();
        let media = MediaStore::open_in_memory().unwrap();
        let mut profile = crate::MediaProfile::new(family_id, "Child");
        profile.user_id = Some(user_id);
        let profile_id = profile.id;
        media.save_profile(&profile).await.unwrap();
        (families, user_id, family_id, media, profile_id)
    }

    #[tokio::test]
    async fn authorization_callback_links_profile_to_encrypted_external_connection() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "short-lived-access",
                "refresh_token": "durable-refresh",
                "expires_in": 7200
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/users/settings"))
            .and(header("Authorization", "Bearer short-lived-access"))
            .and(header(
                "User-Agent",
                concat!("EME/", env!("CARGO_PKG_VERSION")),
            ))
            .and(header("Content-Type", "application/json"))
            .and(header("trakt-api-version", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "user": {"username": "example-user", "name": "Example"}
            })))
            .mount(&server)
            .await;

        let (families, user_id, family_id, media, profile_id) = family_and_profile().await;
        let db_path = std::env::temp_dir().join(format!("media-trakt-{}.db", Uuid::new_v4()));
        let external = Arc::new(SqliteExternalConnectionStore::open(&db_path, [7; 32]).unwrap());
        let external_trait: Arc<dyn ExternalConnectionStore> = external.clone();
        let service = TraktOAuthService::new(
            TraktOAuthConfig::with_endpoints(
                "client",
                "http://localhost/callback",
                format!("{}/authorize", server.uri()),
                format!("{}/oauth/token", server.uri()),
                server.uri(),
            )
            .unwrap(),
            media.clone(),
            families,
            external_trait,
        )
        .unwrap();

        let authorization = service
            .begin_link(user_id, family_id, profile_id)
            .await
            .unwrap();
        let url = Url::parse(&authorization.authorization_url).unwrap();
        let state = url
            .query_pairs()
            .find(|(key, _)| key == "state")
            .unwrap()
            .1
            .into_owned();
        let query = url
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            query
                .get("code_challenge_method")
                .map(|value| value.as_ref()),
            Some("S256")
        );
        assert!(
            !query.contains_key("scope"),
            "Trakt authorization requests must not send an unsupported scope"
        );
        assert!(query.contains_key("code_challenge"));
        let link = service
            .complete_link(user_id, &state, "one-time-code")
            .await
            .unwrap();

        assert_eq!(link.provider_subject, "example-user");
        assert_eq!(link.display_name.as_deref(), Some("Example"));
        assert_eq!(
            media
                .get_external_connection_link(profile_id, "trakt")
                .await
                .unwrap(),
            Some(link.clone())
        );
        assert_eq!(
            external
                .refresh_token(user_id, link.external_connection_id)
                .await
                .unwrap(),
            "durable-refresh"
        );
        assert!(matches!(
            service
                .complete_link(user_id, &state, "one-time-code")
                .await,
            Err(TraktError::MediaStorage(MediaStoreError::PendingNotFound))
        ));

        // The same complete OAuth flow works for a user with no family.
        let personal_user = UserId::new();
        let personal = media
            .get_or_create_personal_profile(personal_user)
            .await
            .unwrap();
        let personal_authorization = service
            .begin_personal_link(personal_user, personal.id)
            .await
            .unwrap();
        let personal_state = Url::parse(&personal_authorization.authorization_url)
            .unwrap()
            .query_pairs()
            .find(|(key, _)| key == "state")
            .unwrap()
            .1
            .into_owned();
        let personal_link = service
            .complete_link(personal_user, &personal_state, "personal-one-time-code")
            .await
            .unwrap();
        assert_eq!(personal_link.owner_id, personal_user);
        assert_eq!(personal_link.family_id, None);
        assert_eq!(
            media
                .get_external_connection_link(personal.id, "trakt")
                .await
                .unwrap(),
            Some(personal_link)
        );

        drop(service);
        drop(external);
        let _ = fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn token_manager_rotates_refresh_tokens_and_reuses_cached_access_tokens() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "cached-access",
                "refresh_token": "rotated-refresh",
                "expires_in": 3600,
                "scope": "private"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let db_path = std::env::temp_dir().join(format!("media-trakt-token-{}.db", Uuid::new_v4()));
        let external = Arc::new(SqliteExternalConnectionStore::open(&db_path, [9; 32]).unwrap());
        let owner_id = UserId::new();
        let connection = external
            .create_connection(NewExternalConnection {
                owner_id,
                provider: "trakt".into(),
                provider_subject: "example-user".into(),
                display_email: None,
                refresh_token: "original-refresh".into(),
                granted_scopes: vec!["public".into()],
            })
            .await
            .unwrap();
        let external_trait: Arc<dyn ExternalConnectionStore> = external.clone();
        let manager = TraktTokenManager::new(
            TraktOAuthConfig::with_endpoints(
                "client",
                "http://localhost/callback",
                format!("{}/authorize", server.uri()),
                format!("{}/oauth/token", server.uri()),
                server.uri(),
            )
            .unwrap(),
            external_trait,
        )
        .unwrap();

        assert_eq!(
            manager.access_token(owner_id, connection.id).await.unwrap(),
            "cached-access"
        );
        assert_eq!(
            manager.access_token(owner_id, connection.id).await.unwrap(),
            "cached-access"
        );
        assert_eq!(
            external
                .refresh_token(owner_id, connection.id)
                .await
                .unwrap(),
            "rotated-refresh"
        );

        drop(manager);
        drop(external);
        let _ = fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn only_the_assigned_user_can_link_their_profile() {
        let (families, user_id, family_id, media, profile_id) = family_and_profile().await;
        let member_id = UserId::new();
        families
            .add_member(FamilyMember {
                family_id,
                user_id: member_id,
                role: FamilyRole::Member,
                relationship: agent_storage::FamilyRelationship::Partner,
                joined_at: Utc::now(),
            })
            .await
            .unwrap();
        let db_path = std::env::temp_dir().join(format!("media-trakt-{}.db", Uuid::new_v4()));
        let external = Arc::new(SqliteExternalConnectionStore::open(&db_path, [8; 32]).unwrap());
        let external_trait: Arc<dyn ExternalConnectionStore> = external.clone();
        let service = TraktOAuthService::new(
            TraktOAuthConfig::new("client", "http://localhost/callback").unwrap(),
            media.clone(),
            families,
            external_trait,
        )
        .unwrap();
        assert!(matches!(
            service.begin_link(member_id, family_id, profile_id).await,
            Err(TraktError::Forbidden)
        ));

        let mut member_profile = crate::MediaProfile::new(family_id, "Member");
        member_profile.user_id = Some(member_id);
        let member_profile_id = member_profile.id;
        media.save_profile(&member_profile).await.unwrap();
        assert!(
            service
                .begin_link(member_id, family_id, member_profile_id)
                .await
                .is_ok(),
            "a regular family member can link their own assigned profile"
        );
        // A user with no family membership can still link their own personal profile.
        let standalone_user = UserId::new();
        let personal = media
            .get_or_create_personal_profile(standalone_user)
            .await
            .unwrap();
        assert!(
            service
                .begin_personal_link(standalone_user, personal.id)
                .await
                .is_ok()
        );
        assert!(matches!(
            service.begin_personal_link(member_id, personal.id).await,
            Err(TraktError::Forbidden)
        ));

        drop(service);
        drop(external);
        let _ = fs::remove_file(db_path);
        let _ = user_id;
    }
}
