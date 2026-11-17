//! Authenticated Google account connection and OAuth callback endpoints.

use std::{path::Path, str::FromStr, sync::Arc};

use actix_web::{
    HttpRequest, HttpResponse, Responder, Scope, delete, get, post,
    web::{self, Data, Json, Path as PathParam, Query},
};
use agent_service::{FamilyService, FamilyServiceError, GoogleCalendarAccessService};
use agent_storage::{
    CalendarAccessMode, CalendarFamilyAccess, ConnectedCalendarId, ConnectedCalendarStorageError,
    ConnectedCalendarStore, DiscoveredCalendar, ExternalConnectionId, ExternalConnectionStatus,
    ExternalConnectionStore, FamilyId, FileUserIdentityStore, NewExternalAuthorization,
    NewExternalConnection, SqliteConnectedCalendarStore, SqliteExternalConnectionStore, UserId,
    UserIdentityStore,
};
use chrono::{Duration, Utc};
use google_tool::{
    CalendarScope, GoogleCalendarRuntimeConfig, GoogleOAuthConfig, authorization_request,
    exchange_code, fetch_user_info, list_calendars,
};
use serde::{Deserialize, Serialize};

const AUTHORIZATION_TTL_MINUTES: i64 = 10;

#[derive(Clone)]
pub struct GoogleOAuthState {
    config: Option<GoogleOAuthConfig>,
    store: Option<SqliteExternalConnectionStore>,
    calendar_store: Option<SqliteConnectedCalendarStore>,
}

impl GoogleOAuthState {
    pub fn from_env(data_dir: &Path) -> Result<Self, String> {
        let client_id = std::env::var("GOOGLE_OAUTH_CLIENT_ID").ok();
        let client_secret = std::env::var("GOOGLE_OAUTH_CLIENT_SECRET").ok();
        let redirect_uri = std::env::var("GOOGLE_OAUTH_REDIRECT_URI").ok();
        let encryption_key = std::env::var("EME_CREDENTIAL_ENCRYPTION_KEY").ok();
        let configured = [
            client_id.is_some(),
            client_secret.is_some(),
            redirect_uri.is_some(),
            encryption_key.is_some(),
        ];
        if configured.iter().all(|value| !*value) {
            return Ok(Self {
                config: None,
                store: None,
                calendar_store: None,
            });
        }
        if configured.iter().any(|value| !*value) {
            return Err("Google OAuth requires GOOGLE_OAUTH_CLIENT_ID, GOOGLE_OAUTH_CLIENT_SECRET, GOOGLE_OAUTH_REDIRECT_URI, and EME_CREDENTIAL_ENCRYPTION_KEY".to_owned());
        }
        let key = parse_encryption_key(encryption_key.as_deref().unwrap_or_default())?;
        let database_path = data_dir.join("external_connections.sqlite");
        let store = SqliteExternalConnectionStore::open(&database_path, key)
            .map_err(|error| format!("opening external connection storage: {error}"))?;
        let calendar_store = SqliteConnectedCalendarStore::open(&database_path)
            .map_err(|error| format!("opening connected calendar storage: {error}"))?;
        Ok(Self {
            config: Some(GoogleOAuthConfig {
                client_id: client_id.unwrap_or_default(),
                client_secret: client_secret.unwrap_or_default(),
                redirect_uri: redirect_uri.unwrap_or_default(),
            }),
            store: Some(store),
            calendar_store: Some(calendar_store),
        })
    }

    fn enabled(&self) -> bool {
        self.config.is_some() && self.store.is_some() && self.calendar_store.is_some()
    }

    /// Compose the trusted Google Calendar tool runtime from the same stores
    /// used by the authenticated OAuth and calendar-management endpoints.
    pub fn calendar_runtime(
        &self,
        families: Arc<FamilyService>,
    ) -> Option<GoogleCalendarRuntimeConfig> {
        let oauth = self.config.clone()?;
        let connections = self.store.clone()?;
        let calendars = self.calendar_store.clone()?;
        let access = Arc::new(GoogleCalendarAccessService::new(
            Arc::new(calendars),
            Arc::new(connections),
            families,
        ));
        Some(GoogleCalendarRuntimeConfig::new(oauth, access))
    }
}

fn parse_encryption_key(value: &str) -> Result<[u8; 32], String> {
    if value.len() != 64 || !value.is_ascii() {
        return Err(
            "EME_CREDENTIAL_ENCRYPTION_KEY must contain exactly 64 hexadecimal characters"
                .to_owned(),
        );
    }
    let mut key = [0_u8; 32];
    for (index, byte) in key.iter_mut().enumerate() {
        let start = index * 2;
        *byte = u8::from_str_radix(&value[start..start + 2], 16)
            .map_err(|_| "EME_CREDENTIAL_ENCRYPTION_KEY must be hexadecimal".to_owned())?;
    }
    Ok(key)
}

fn current_user_id(
    request: &HttpRequest,
    auth: &crate::auth::TelegramAuth,
    identities: &FileUserIdentityStore,
) -> Result<UserId, HttpResponse> {
    let owner = auth
        .owner_for_request(request)
        .ok_or_else(|| HttpResponse::Unauthorized().finish())?;
    let subject = owner
        .strip_prefix("telegram:")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| HttpResponse::Unauthorized().finish())?;
    let identity = identities
        .find_identity("telegram", subject)
        .map_err(|_| HttpResponse::InternalServerError().finish())?
        .ok_or_else(|| HttpResponse::Unauthorized().finish())?;
    Ok(identity.user_id)
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RequestedAccess {
    ReadOnly,
    ReadWrite,
}

impl From<RequestedAccess> for CalendarScope {
    fn from(value: RequestedAccess) -> Self {
        match value {
            RequestedAccess::ReadOnly => Self::ReadOnly,
            RequestedAccess::ReadWrite => Self::ReadWrite,
        }
    }
}

#[derive(Deserialize)]
struct StartGoogleOAuthRequest {
    access: RequestedAccess,
}

#[derive(Serialize)]
struct StartGoogleOAuthResponse {
    authorization_url: String,
    expires_in_seconds: u64,
}

#[post("/oauth/start")]
async fn start_oauth(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    state: Data<GoogleOAuthState>,
    body: Json<StartGoogleOAuthRequest>,
) -> impl Responder {
    if !state.enabled() {
        return HttpResponse::ServiceUnavailable().body("Google OAuth is not configured");
    }
    let user_id = match current_user_id(&request, &auth, &identities) {
        Ok(id) => id,
        Err(response) => return response,
    };
    let config = state
        .config
        .as_ref()
        .expect("enabled state has OAuth config");
    let store = state.store.as_ref().expect("enabled state has storage");
    let access: CalendarScope = body.access.into();
    let requested_scopes = match access {
        CalendarScope::ReadOnly => vec![
            "https://www.googleapis.com/auth/calendar.calendarlist.readonly".to_owned(),
            "https://www.googleapis.com/auth/calendar.events.readonly".to_owned(),
            "openid".to_owned(),
            "email".to_owned(),
        ],
        CalendarScope::ReadWrite => vec![
            "https://www.googleapis.com/auth/calendar.calendarlist.readonly".to_owned(),
            "https://www.googleapis.com/auth/calendar.events".to_owned(),
            "openid".to_owned(),
            "email".to_owned(),
        ],
    };
    let pending = match authorization_request(config, access) {
        Ok(value) => value,
        Err(error) => {
            tracing::error!(%error, "could not create Google OAuth request");
            return HttpResponse::BadGateway().finish();
        }
    };
    let expires_at = Utc::now() + Duration::minutes(AUTHORIZATION_TTL_MINUTES);
    if let Err(error) = store
        .begin_authorization(NewExternalAuthorization {
            user_id,
            provider: "google".to_owned(),
            state: pending.state,
            pkce_verifier: pending.pkce_verifier,
            requested_scopes,
            expires_at,
        })
        .await
    {
        tracing::error!(%error, "could not persist Google OAuth state");
        return HttpResponse::InternalServerError().finish();
    }
    HttpResponse::Created().json(StartGoogleOAuthResponse {
        authorization_url: pending.authorization_url.to_string(),
        expires_in_seconds: (AUTHORIZATION_TTL_MINUTES * 60) as u64,
    })
}

#[derive(Deserialize)]
struct GoogleOAuthCallback {
    code: Option<String>,
    state: String,
    error: Option<String>,
}

#[derive(Serialize)]
struct ConnectedGoogleAccount {
    id: ExternalConnectionId,
    email: Option<String>,
    scopes: Vec<String>,
    status: ExternalConnectionStatus,
    updated_at: chrono::DateTime<Utc>,
}

#[get("/oauth/callback")]
async fn oauth_callback(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    state: Data<GoogleOAuthState>,
    query: Query<GoogleOAuthCallback>,
) -> impl Responder {
    if !state.enabled() {
        return HttpResponse::ServiceUnavailable().body("Google OAuth is not configured");
    }
    let user_id = match current_user_id(&request, &auth, &identities) {
        Ok(id) => id,
        Err(response) => return response,
    };
    let store = state.store.as_ref().expect("enabled state has storage");
    let consumed = match store
        .consume_authorization("google", &query.state, Utc::now())
        .await
    {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(%error, "rejected Google OAuth callback state");
            return HttpResponse::BadRequest().body("Invalid or expired OAuth state");
        }
    };
    if consumed.user_id != user_id {
        return HttpResponse::Forbidden().body("OAuth authorization belongs to another user");
    }
    if query.error.is_some() {
        return HttpResponse::BadRequest().body("Google authorization was not completed");
    }
    let Some(code) = query.code.as_ref().filter(|value| !value.is_empty()) else {
        return HttpResponse::BadRequest().body("Google did not return an authorization code");
    };
    let config = state
        .config
        .as_ref()
        .expect("enabled state has OAuth config");
    let tokens = match exchange_code(config, code.clone(), consumed.pkce_verifier).await {
        Ok(tokens) => tokens,
        Err(error) => {
            tracing::warn!(%error, "Google OAuth code exchange failed");
            return HttpResponse::BadGateway().body("Google token exchange failed");
        }
    };
    let identity = match fetch_user_info(&tokens.access_token).await {
        Ok(identity) if identity.email_verified && !identity.sub.is_empty() => identity,
        Ok(_) => return HttpResponse::BadRequest().body("Google account identity is not verified"),
        Err(error) => {
            tracing::warn!(%error, "Google account identity verification failed");
            return HttpResponse::BadGateway().body("Could not verify Google account identity");
        }
    };
    let existing = match store.list_connections(user_id).await {
        Ok(connections) => connections.into_iter().find(|connection| {
            connection.provider == "google" && connection.provider_subject == identity.sub
        }),
        Err(error) => {
            tracing::error!(%error, "could not look up existing Google connection");
            return HttpResponse::InternalServerError().finish();
        }
    };
    // Google may omit a refresh token on subsequent consent grants. Reuse the
    // encrypted token only for this same verified provider subject.
    let refresh_token = match tokens.refresh_token.clone() {
        Some(token) => token,
        None => {
            let Some(existing) = existing.as_ref() else {
                return HttpResponse::Conflict()
                    .body("Google did not issue a refresh token for this new account");
            };
            match store.refresh_token(user_id, existing.id).await {
                Ok(token) => token,
                Err(error) => {
                    tracing::error!(%error, "could not recover existing Google refresh token");
                    return HttpResponse::InternalServerError().finish();
                }
            }
        }
    };
    let connection = match store
        .create_connection(NewExternalConnection {
            owner_id: user_id,
            provider: "google".to_owned(),
            provider_subject: identity.sub.clone(),
            display_email: Some(identity.email.clone()),
            refresh_token,
            granted_scopes: tokens.scopes.clone(),
        })
        .await
    {
        Ok(connection) => connection,
        Err(agent_storage::ExternalConnectionStorageError::AlreadyExists) => {
            let Some(existing) = existing else {
                return HttpResponse::Conflict().body("This Google account is already connected");
            };
            match store
                .update_credentials(user_id, existing.id, tokens.refresh_token, tokens.scopes)
                .await
            {
                Ok(connection) => connection,
                Err(error) => {
                    tracing::error!(%error, "could not update existing Google connection");
                    return HttpResponse::InternalServerError().finish();
                }
            }
        }
        Err(error) => {
            tracing::error!(%error, "could not persist Google account connection");
            return HttpResponse::InternalServerError().finish();
        }
    };
    HttpResponse::Created().json(ConnectedGoogleAccount {
        id: connection.id,
        email: connection.display_email,
        scopes: connection.granted_scopes,
        status: connection.status,
        updated_at: connection.updated_at,
    })
}

#[get("/connections")]
async fn list_connections(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    state: Data<GoogleOAuthState>,
) -> impl Responder {
    if !state.enabled() {
        return HttpResponse::ServiceUnavailable().body("Google OAuth is not configured");
    }
    let user_id = match current_user_id(&request, &auth, &identities) {
        Ok(id) => id,
        Err(response) => return response,
    };
    match state
        .store
        .as_ref()
        .expect("enabled state has storage")
        .list_connections(user_id)
        .await
    {
        Ok(connections) => HttpResponse::Ok().json(
            connections
                .into_iter()
                .map(|connection| ConnectedGoogleAccount {
                    id: connection.id,
                    email: connection.display_email,
                    scopes: connection.granted_scopes,
                    status: connection.status,
                    updated_at: connection.updated_at,
                })
                .collect::<Vec<_>>(),
        ),
        Err(error) => {
            tracing::error!(%error, "could not list Google account connections");
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[delete("/connections/{connection_id}")]
async fn disconnect(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    state: Data<GoogleOAuthState>,
    connection_id: PathParam<String>,
) -> impl Responder {
    if !state.enabled() {
        return HttpResponse::ServiceUnavailable().body("Google OAuth is not configured");
    }
    let user_id = match current_user_id(&request, &auth, &identities) {
        Ok(id) => id,
        Err(response) => return response,
    };
    let id = match ExternalConnectionId::from_str(&connection_id) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().body("Invalid connection ID"),
    };
    let store = state.store.as_ref().expect("enabled state has storage");
    match store.get_connection(user_id, id).await {
        Ok(Some(connection)) if connection.status == ExternalConnectionStatus::Active => {}
        Ok(_) => return HttpResponse::NotFound().finish(),
        Err(error) => {
            tracing::error!(%error, "could not inspect Google connection before disconnect");
            return HttpResponse::InternalServerError().finish();
        }
    }
    let refresh_token = match store.refresh_token(user_id, id).await {
        Ok(token) => token,
        Err(agent_storage::ExternalConnectionStorageError::NotFound) => {
            return HttpResponse::NotFound().finish();
        }
        Err(error) => {
            tracing::error!(%error, "could not load Google credential for revocation");
            return HttpResponse::InternalServerError().finish();
        }
    };
    if let Err(error) = store.revoke_connection(user_id, id).await {
        tracing::error!(%error, "could not revoke local Google connection");
        return HttpResponse::InternalServerError().finish();
    }
    // Local access is disabled first and remains disabled even if Google's
    // revocation endpoint is temporarily unavailable.
    if let Err(error) = google_tool::revoke_token(&refresh_token).await {
        tracing::warn!(%error, "local Google connection revoked, but provider token revocation failed");
    }
    HttpResponse::NoContent().finish()
}

#[derive(Serialize)]
struct ConnectedCalendarResponse {
    id: ConnectedCalendarId,
    display_name: Option<String>,
    time_zone: Option<String>,
    provider_access_role: Option<String>,
    selected: bool,
    access_mode: CalendarAccessMode,
    family_id: Option<FamilyId>,
    family_access: Option<CalendarFamilyAccess>,
}

impl From<agent_storage::ConnectedCalendar> for ConnectedCalendarResponse {
    fn from(calendar: agent_storage::ConnectedCalendar) -> Self {
        Self {
            id: calendar.id,
            display_name: calendar.display_name,
            time_zone: calendar.time_zone,
            provider_access_role: calendar.provider_access_role,
            selected: calendar.selected,
            access_mode: calendar.access_mode,
            family_id: calendar.family_id,
            family_access: calendar.family_access,
        }
    }
}

fn calendar_storage_error(error: ConnectedCalendarStorageError) -> HttpResponse {
    match error {
        ConnectedCalendarStorageError::ConnectionNotFound
        | ConnectedCalendarStorageError::CalendarNotFound => HttpResponse::NotFound().finish(),
        ConnectedCalendarStorageError::InsufficientScopes => HttpResponse::Conflict()
            .body("The connected Google account has not granted calendar write access"),
        ConnectedCalendarStorageError::EmptyProviderCalendarId => {
            HttpResponse::BadRequest().finish()
        }
        ConnectedCalendarStorageError::FamilyShareRequiresSelected => {
            HttpResponse::Conflict().body("Select the calendar before sharing it")
        }
        ConnectedCalendarStorageError::FamilyWriteRequiresOwnerWrite => {
            HttpResponse::Conflict().body("Contributor sharing requires owner read/write access")
        }
        ConnectedCalendarStorageError::Backend(error) => {
            tracing::error!(%error, "connected calendar storage failed");
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[get("/connections/{connection_id}/calendars")]
async fn list_connected_calendars(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    state: Data<GoogleOAuthState>,
    connection_id: PathParam<String>,
) -> impl Responder {
    if !state.enabled() {
        return HttpResponse::ServiceUnavailable().body("Google OAuth is not configured");
    }
    let owner = match current_user_id(&request, &auth, &identities) {
        Ok(id) => id,
        Err(response) => return response,
    };
    let connection_id = match ExternalConnectionId::from_str(&connection_id) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().body("Invalid connection ID"),
    };
    let calendars = state
        .calendar_store
        .as_ref()
        .expect("enabled state has calendar storage")
        .list_for_connection(owner, connection_id)
        .await;
    match calendars {
        Ok(calendars) => HttpResponse::Ok().json(
            calendars
                .into_iter()
                .map(ConnectedCalendarResponse::from)
                .collect::<Vec<_>>(),
        ),
        Err(error) => calendar_storage_error(error),
    }
}

#[post("/connections/{connection_id}/calendars/sync")]
async fn sync_connected_calendars(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    state: Data<GoogleOAuthState>,
    connection_id: PathParam<String>,
) -> impl Responder {
    if !state.enabled() {
        return HttpResponse::ServiceUnavailable().body("Google OAuth is not configured");
    }
    let owner = match current_user_id(&request, &auth, &identities) {
        Ok(id) => id,
        Err(response) => return response,
    };
    let connection_id = match ExternalConnectionId::from_str(&connection_id) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().body("Invalid connection ID"),
    };
    let store = state
        .store
        .as_ref()
        .expect("enabled state has connection storage");
    let connection = match store.get_connection(owner, connection_id).await {
        Ok(Some(connection))
            if connection.provider == "google"
                && connection.status == ExternalConnectionStatus::Active =>
        {
            connection
        }
        Ok(_) => return HttpResponse::NotFound().finish(),
        Err(error) => {
            tracing::error!(%error, "could not inspect Google connection for calendar sync");
            return HttpResponse::InternalServerError().finish();
        }
    };
    let refresh_token = match store.refresh_token(owner, connection_id).await {
        Ok(token) => token,
        Err(error) => {
            tracing::error!(%error, "could not load Google credential for calendar sync");
            return HttpResponse::InternalServerError().finish();
        }
    };
    let config = state
        .config
        .as_ref()
        .expect("enabled state has OAuth config");
    let discovered = match list_calendars(config, &refresh_token).await {
        Ok(calendars) => calendars
            .into_iter()
            .map(|calendar| DiscoveredCalendar {
                provider_calendar_id: calendar.id,
                display_name: calendar.summary,
                time_zone: calendar.time_zone,
                provider_access_role: calendar.access_role,
            })
            .collect(),
        Err(error) => {
            tracing::warn!(%error, connection_id = %connection.id, "Google Calendar discovery failed");
            return HttpResponse::BadGateway().body("Google Calendar discovery failed");
        }
    };
    let calendars = state
        .calendar_store
        .as_ref()
        .expect("enabled state has calendar storage")
        .sync_discovered(owner, connection_id, discovered)
        .await;
    match calendars {
        Ok(calendars) => HttpResponse::Ok().json(
            calendars
                .into_iter()
                .map(ConnectedCalendarResponse::from)
                .collect::<Vec<_>>(),
        ),
        Err(error) => calendar_storage_error(error),
    }
}

#[derive(Deserialize)]
struct UpdateConnectedCalendarRequest {
    selected: Option<bool>,
    access_mode: Option<CalendarAccessMode>,
}

#[actix_web::patch("/calendars/{calendar_id}")]
async fn update_connected_calendar(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    state: Data<GoogleOAuthState>,
    calendar_id: PathParam<String>,
    body: Json<UpdateConnectedCalendarRequest>,
) -> impl Responder {
    if !state.enabled() {
        return HttpResponse::ServiceUnavailable().body("Google OAuth is not configured");
    }
    if body.selected.is_none() && body.access_mode.is_none() {
        return HttpResponse::BadRequest().body("At least one calendar setting must be supplied");
    }
    let owner = match current_user_id(&request, &auth, &identities) {
        Ok(id) => id,
        Err(response) => return response,
    };
    let calendar_id = match ConnectedCalendarId::from_str(&calendar_id) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().body("Invalid calendar ID"),
    };
    let calendar = state
        .calendar_store
        .as_ref()
        .expect("enabled state has calendar storage")
        .update(owner, calendar_id, body.selected, body.access_mode)
        .await;
    match calendar {
        Ok(calendar) => HttpResponse::Ok().json(ConnectedCalendarResponse::from(calendar)),
        Err(error) => calendar_storage_error(error),
    }
}

#[derive(Deserialize)]
struct ShareCalendarWithFamilyRequest {
    family_id: String,
    access: CalendarFamilyAccess,
}

fn family_sharing_error(error: FamilyServiceError) -> HttpResponse {
    match error {
        FamilyServiceError::NotFound | FamilyServiceError::NotMember => {
            HttpResponse::NotFound().finish()
        }
        FamilyServiceError::Forbidden => HttpResponse::Forbidden().finish(),
        other => {
            tracing::error!(%other, "could not validate family membership for calendar sharing");
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[actix_web::put("/calendars/{calendar_id}/family-sharing")]
async fn share_calendar_with_family(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    state: Data<GoogleOAuthState>,
    families: Data<Arc<FamilyService>>,
    calendar_id: PathParam<String>,
    body: Json<ShareCalendarWithFamilyRequest>,
) -> impl Responder {
    if !state.enabled() {
        return HttpResponse::ServiceUnavailable().body("Google OAuth is not configured");
    }
    let owner = match current_user_id(&request, &auth, &identities) {
        Ok(id) => id,
        Err(response) => return response,
    };
    let calendar_id = match ConnectedCalendarId::from_str(&calendar_id) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().body("Invalid calendar ID"),
    };
    let family_id = match FamilyId::from_str(&body.family_id) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().body("Invalid family ID"),
    };
    // Sharing can only be configured with a family the calendar owner currently belongs to.
    if let Err(error) = families.context_for_member(owner, family_id).await {
        return family_sharing_error(error);
    }
    let calendar = state
        .calendar_store
        .as_ref()
        .expect("enabled state has calendar storage")
        .share_with_family(owner, calendar_id, family_id, body.access)
        .await;
    match calendar {
        Ok(calendar) => HttpResponse::Ok().json(ConnectedCalendarResponse::from(calendar)),
        Err(error) => calendar_storage_error(error),
    }
}

#[actix_web::delete("/calendars/{calendar_id}/family-sharing")]
async fn unshare_calendar_from_family(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    state: Data<GoogleOAuthState>,
    calendar_id: PathParam<String>,
) -> impl Responder {
    if !state.enabled() {
        return HttpResponse::ServiceUnavailable().body("Google OAuth is not configured");
    }
    let owner = match current_user_id(&request, &auth, &identities) {
        Ok(id) => id,
        Err(response) => return response,
    };
    let calendar_id = match ConnectedCalendarId::from_str(&calendar_id) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().body("Invalid calendar ID"),
    };
    let calendar = state
        .calendar_store
        .as_ref()
        .expect("enabled state has calendar storage")
        .unshare_from_family(owner, calendar_id)
        .await;
    match calendar {
        Ok(calendar) => HttpResponse::Ok().json(ConnectedCalendarResponse::from(calendar)),
        Err(error) => calendar_storage_error(error),
    }
}

pub fn google_integrations_api() -> Scope {
    web::scope("/integrations/google")
        .service(start_oauth)
        .service(oauth_callback)
        .service(list_connections)
        .service(disconnect)
        .service(list_connected_calendars)
        .service(sync_connected_calendars)
        .service(update_connected_calendar)
        .service(share_calendar_with_family)
        .service(unshare_calendar_from_family)
}

#[cfg(test)]
mod tests {
    use super::parse_encryption_key;

    #[test]
    fn parses_exactly_32_bytes_of_hex_key_material() {
        assert_eq!(parse_encryption_key(&"ab".repeat(32)).unwrap(), [0xab; 32]);
        assert!(parse_encryption_key("abcd").is_err());
        assert!(parse_encryption_key(&"xz".repeat(32)).is_err());
    }
}
