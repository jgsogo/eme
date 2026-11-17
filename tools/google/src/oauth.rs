//! Google OAuth 2.0 Authorization Code flow helpers.
//!
//! This module deliberately exposes the state and PKCE verifier to the caller:
//! the application must persist both, bound to the authenticated EME user,
//! consume them once on callback, and validate state before exchanging a code.
//! This crate does not own application sessions or credential persistence.

use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, RefreshToken, Scope, TokenResponse, TokenUrl,
    basic::BasicClient,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const CALENDAR_LIST_READONLY: &str =
    "https://www.googleapis.com/auth/calendar.calendarlist.readonly";
const CALENDAR_EVENTS_READONLY: &str = "https://www.googleapis.com/auth/calendar.events.readonly";
const CALENDAR_EVENTS: &str = "https://www.googleapis.com/auth/calendar.events";

/// Credentials and callback configuration for one Google OAuth application.
#[derive(Clone)]
pub struct GoogleOAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

/// Minimum EME permission requested from Google.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalendarScope {
    ReadOnly,
    ReadWrite,
}

/// Values that the server must persist for a pending authorization attempt.
///
/// Treat `state` and `pkce_verifier` as secrets. Never log or send them to
/// the browser except for the authorization URL containing the state value.
pub struct PendingAuthorization {
    pub authorization_url: Url,
    pub state: String,
    pub pkce_verifier: String,
}

/// Token material returned by Google's token endpoint.
///
/// Refresh tokens are only returned when Google issues one; callers must keep
/// an existing refresh token when reauthorization omits a replacement.
#[derive(Clone, Serialize, Deserialize)]
pub struct GoogleTokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in_seconds: Option<u64>,
    pub scopes: Vec<String>,
}

#[derive(Debug, Error)]
pub enum GoogleOAuthError {
    #[error("invalid Google OAuth configuration: {0}")]
    Configuration(String),
    #[error("Google OAuth request failed: {0}")]
    Request(String),
}

fn client_id(config: &GoogleOAuthConfig) -> ClientId {
    ClientId::new(config.client_id.clone())
}

fn client_secret(config: &GoogleOAuthConfig) -> ClientSecret {
    ClientSecret::new(config.client_secret.clone())
}

fn auth_url() -> Result<AuthUrl, GoogleOAuthError> {
    AuthUrl::new(GOOGLE_AUTH_URL.to_owned())
        .map_err(|error| GoogleOAuthError::Configuration(error.to_string()))
}

fn token_url() -> Result<TokenUrl, GoogleOAuthError> {
    TokenUrl::new(GOOGLE_TOKEN_URL.to_owned())
        .map_err(|error| GoogleOAuthError::Configuration(error.to_string()))
}

fn redirect_url(config: &GoogleOAuthConfig) -> Result<RedirectUrl, GoogleOAuthError> {
    RedirectUrl::new(config.redirect_uri.clone())
        .map_err(|error| GoogleOAuthError::Configuration(error.to_string()))
}

fn http_client() -> Result<oauth2::reqwest::Client, GoogleOAuthError> {
    oauth2::reqwest::ClientBuilder::new()
        // OAuth token responses must not be redirected to an arbitrary host.
        .redirect(oauth2::reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| GoogleOAuthError::Request(error.to_string()))
}

/// Build a Google consent URL and fresh CSRF/PKCE values.
///
/// The caller must durably store `state` and `pkce_verifier` with the
/// initiating EME user and a short expiry before redirecting the browser.
pub fn authorization_request(
    config: &GoogleOAuthConfig,
    scope: CalendarScope,
) -> Result<PendingAuthorization, GoogleOAuthError> {
    let client = BasicClient::new(client_id(config))
        .set_client_secret(client_secret(config))
        .set_auth_uri(auth_url()?)
        .set_token_uri(token_url()?)
        .set_redirect_uri(redirect_url(config)?);
    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let mut request = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new(CALENDAR_LIST_READONLY.to_owned()))
        .add_extra_param("access_type", "offline")
        .add_extra_param("include_granted_scopes", "true")
        .add_extra_param("prompt", "consent")
        .add_scope(Scope::new("openid".to_owned()))
        .add_scope(Scope::new("email".to_owned()))
        .set_pkce_challenge(challenge);
    request = match scope {
        CalendarScope::ReadOnly => {
            request.add_scope(Scope::new(CALENDAR_EVENTS_READONLY.to_owned()))
        }
        CalendarScope::ReadWrite => request.add_scope(Scope::new(CALENDAR_EVENTS.to_owned())),
    };
    let (url, state) = request.url();
    Ok(PendingAuthorization {
        authorization_url: url,
        state: state.secret().clone(),
        pkce_verifier: verifier.secret().clone(),
    })
}

/// Exchange a validated authorization code for Google tokens.
pub async fn exchange_code(
    config: &GoogleOAuthConfig,
    code: String,
    pkce_verifier: String,
) -> Result<GoogleTokenSet, GoogleOAuthError> {
    let client = BasicClient::new(client_id(config))
        .set_client_secret(client_secret(config))
        .set_auth_uri(auth_url()?)
        .set_token_uri(token_url()?)
        .set_redirect_uri(redirect_url(config)?);
    let token = client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(PkceCodeVerifier::new(pkce_verifier))
        .request_async(&http_client()?)
        .await
        .map_err(|error| GoogleOAuthError::Request(error.to_string()))?;
    Ok(token_set(&token))
}

/// Refresh an access token using a previously stored refresh token.
pub async fn refresh_access_token(
    config: &GoogleOAuthConfig,
    refresh_token: &str,
) -> Result<GoogleTokenSet, GoogleOAuthError> {
    let client = BasicClient::new(client_id(config))
        .set_client_secret(client_secret(config))
        .set_auth_uri(auth_url()?)
        .set_token_uri(token_url()?);
    let token = client
        .exchange_refresh_token(&RefreshToken::new(refresh_token.to_owned()))
        .request_async(&http_client()?)
        .await
        .map_err(|error| GoogleOAuthError::Request(error.to_string()))?;
    Ok(token_set(&token))
}

fn token_set<T: TokenResponse>(token: &T) -> GoogleTokenSet {
    GoogleTokenSet {
        access_token: token.access_token().secret().clone(),
        refresh_token: token.refresh_token().map(|value| value.secret().clone()),
        expires_in_seconds: token.expires_in().map(|value| value.as_secs()),
        scopes: token
            .scopes()
            .map(|scopes| scopes.iter().map(|scope| scope.to_string()).collect())
            .unwrap_or_default(),
    }
}


/// Verified account identity returned by Google's OpenID Connect user-info endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct GoogleUserInfo {
    pub sub: String,
    pub email: String,
    pub email_verified: bool,
}

/// Resolve the Google account associated with an access token.
pub async fn fetch_user_info(access_token: &str) -> Result<GoogleUserInfo, GoogleOAuthError> {
    let client = oauth2::reqwest::ClientBuilder::new()
        .redirect(oauth2::reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| GoogleOAuthError::Request(error.to_string()))?;
    let response = client
        .get("https://openidconnect.googleapis.com/v1/userinfo")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|error| GoogleOAuthError::Request(error.to_string()))?
        .error_for_status()
        .map_err(|error| GoogleOAuthError::Request(error.to_string()))?;
    response
        .json::<GoogleUserInfo>()
        .await
        .map_err(|error| GoogleOAuthError::Request(error.to_string()))
}


/// Revoke a Google OAuth token using Google's fixed revocation endpoint.
pub async fn revoke_token(token: &str) -> Result<(), GoogleOAuthError> {
    let client = oauth2::reqwest::ClientBuilder::new()
        .redirect(oauth2::reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| GoogleOAuthError::Request(error.to_string()))?;
    client
        .post("https://oauth2.googleapis.com/revoke")
        .form(&[("token", token)])
        .send()
        .await
        .map_err(|error| GoogleOAuthError::Request(error.to_string()))?
        .error_for_status()
        .map_err(|error| GoogleOAuthError::Request(error.to_string()))?;
    Ok(())
}
