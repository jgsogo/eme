//! Authenticated Trakt account-linking and synchronization endpoints.
use std::collections::HashSet;
use std::{str::FromStr, sync::Arc};

use actix_web::{
    HttpRequest, HttpResponse, Responder, Scope, delete, get, post,
    web::{self, Data, Json, Path as PathParam, Query},
};
use agent_storage::{FamilyId, FamilyStore, FileUserIdentityStore, UserId, UserIdentityStore};
use media_tools::{
    MediaProfileId, MediaStore, TraktError, TraktOAuthService, TraktSyncError, TraktSyncService,
};
use serde::Deserialize;
use serde_json::json;

use crate::auth::TelegramAuth;

/// Runtime state shared by media integration routes.
#[derive(Clone, Default)]
pub struct MediaApiState {
    pub media: Option<MediaStore>,
    pub families: Option<Arc<dyn FamilyStore>>,
    pub trakt_oauth: Option<Arc<TraktOAuthService>>,
    pub trakt_sync: Option<TraktSyncService>,
}

#[derive(Debug, Deserialize)]
pub struct TraktLinkRequest {
    pub family_id: Option<String>,
    pub profile_id: String,
}

#[derive(Debug, Deserialize)]
pub struct TraktCallbackQuery {
    pub state: String,
    pub code: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TraktProfileRequest {
    pub family_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TraktSyncQuery {
    pub family_id: Option<String>,
}

#[get("/trakt/profiles")]
async fn list_trakt_profiles(
    request: HttpRequest,
    auth: Data<TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    state: Data<MediaApiState>,
) -> impl Responder {
    let user_id = match current_user_id(&request, &auth, &identities) {
        Ok(user_id) => user_id,
        Err(response) => return response,
    };
    let Some(media) = state.media.as_ref() else {
        return HttpResponse::ServiceUnavailable().body("Media storage is not configured");
    };
    let personal = match media.get_or_create_personal_profile(user_id).await {
        Ok(profile) => profile,
        Err(error) => return HttpResponse::InternalServerError().body(error.to_string()),
    };
    if state.trakt_oauth.is_none() {
        return HttpResponse::ServiceUnavailable().body("Trakt OAuth is not configured");
    }
    let mut profiles = Vec::new();
    let personal_link = match media
        .get_external_connection_link(personal.id, "trakt")
        .await
    {
        Ok(link) => link,
        Err(error) => return HttpResponse::InternalServerError().body(error.to_string()),
    };
    let personal_sync = match media.get_provider_sync_state(personal.id, "trakt").await {
        Ok(sync) => sync,
        Err(error) => return HttpResponse::InternalServerError().body(error.to_string()),
    };
    profiles.push(json!({
        "profile_id": personal.id,
        "profile_name": personal.name,
        "family_id": null,
        "family_name": "Personal",
        "trakt": personal_link.map(|link| json!({
            "username": link.provider_subject,
            "display_name": link.display_name,
            "linked_at": link.linked_at,
        })),
        "sync": personal_sync,
    }));
    let memberships = if let Some(families) = state.families.as_ref() {
        match families.list_families(user_id).await {
            Ok(families) => families,
            Err(error) => return HttpResponse::InternalServerError().body(error.to_string()),
        }
    } else {
        Vec::new()
    };
    for family in memberships {
        let family_profiles = match media.list_profiles(family.id).await {
            Ok(profiles) => profiles,
            Err(error) => return HttpResponse::InternalServerError().body(error.to_string()),
        };
        for profile in family_profiles
            .into_iter()
            .filter(|profile| profile.active && profile.user_id == Some(user_id))
        {
            let link = match media
                .get_external_connection_link(profile.id, "trakt")
                .await
            {
                Ok(link) => link,
                Err(error) => return HttpResponse::InternalServerError().body(error.to_string()),
            };
            let sync = match media.get_provider_sync_state(profile.id, "trakt").await {
                Ok(sync) => sync,
                Err(error) => return HttpResponse::InternalServerError().body(error.to_string()),
            };
            profiles.push(json!({
                "profile_id": profile.id,
                "profile_name": profile.name,
                "family_id": family.id,
                "family_name": family.name.clone(),
                "trakt": link.map(|link| json!({
                    "username": link.provider_subject,
                    "display_name": link.display_name,
                    "linked_at": link.linked_at,
                })),
                "sync": sync,
            }));
        }
    }
    HttpResponse::Ok().json(profiles)
}

#[post("/trakt/link")]
async fn start_trakt_link(
    request: HttpRequest,
    auth: Data<TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    state: Data<MediaApiState>,
    body: Json<TraktLinkRequest>,
) -> impl Responder {
    let Some(oauth) = state.trakt_oauth.as_ref() else {
        return HttpResponse::ServiceUnavailable().body("Trakt OAuth is not configured");
    };
    let owner_id = match current_user_id(&request, &auth, &identities) {
        Ok(user_id) => user_id,
        Err(response) => return response,
    };
    let profile_id = match MediaProfileId::from_str(&body.profile_id) {
        Ok(id) => id,
        Err(error) => return HttpResponse::BadRequest().body(error.to_string()),
    };
    let result = match body.family_id.as_deref() {
        Some(value) => match parse_id::<FamilyId>(value) {
            Ok(family_id) => oauth.begin_link(owner_id, family_id, profile_id).await,
            Err(error) => return HttpResponse::BadRequest().body(error),
        },
        None => oauth.begin_personal_link(owner_id, profile_id).await,
    };
    match result {
        Ok(authorization) => HttpResponse::Ok().json(authorization),
        Err(error) => trakt_error_response(error),
    }
}

#[get("/trakt/callback")]
async fn complete_trakt_link(
    request: HttpRequest,
    auth: Data<TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    state: Data<MediaApiState>,
    query: Query<TraktCallbackQuery>,
) -> impl Responder {
    let Some(oauth) = state.trakt_oauth.as_ref() else {
        return HttpResponse::ServiceUnavailable().body("Trakt OAuth is not configured");
    };
    if let Some(error) = query.error.as_deref() {
        return HttpResponse::BadRequest().body(format!("Trakt authorization was denied: {error}"));
    }
    let Some(code) = query.code.as_deref() else {
        return HttpResponse::BadRequest()
            .body("Trakt callback did not include an authorization code");
    };
    let owner_id = match current_user_id(&request, &auth, &identities) {
        Ok(user_id) => user_id,
        Err(response) => return response,
    };
    match oauth.complete_link(owner_id, &query.state, code).await {
        Ok(link) => HttpResponse::Ok().json(link),
        Err(error) => trakt_error_response(error),
    }
}

#[get("/trakt/profiles/{profile_id}/summary")]
async fn trakt_profile_summary(
    request: HttpRequest,
    auth: Data<TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    state: Data<MediaApiState>,
    profile_id: PathParam<String>,
    query: Query<TraktSyncQuery>,
) -> impl Responder {
    let owner_id = match current_user_id(&request, &auth, &identities) {
        Ok(user_id) => user_id,
        Err(response) => return response,
    };
    let profile_id = match MediaProfileId::from_str(&profile_id) {
        Ok(id) => id,
        Err(error) => return HttpResponse::BadRequest().body(error.to_string()),
    };
    let Some(media) = state.media.as_ref() else {
        return HttpResponse::ServiceUnavailable().body("Media storage is not configured");
    };
    let Some(oauth) = state.trakt_oauth.as_ref() else {
        return HttpResponse::ServiceUnavailable().body("Trakt OAuth is not configured");
    };
    let validation = match query.family_id.as_deref() {
        Some(value) => match parse_id::<FamilyId>(value) {
            Ok(family_id) => {
                oauth
                    .validate_profile_owner(owner_id, family_id, profile_id)
                    .await
            }
            Err(error) => return HttpResponse::BadRequest().body(error),
        },
        None => {
            oauth
                .validate_personal_profile_owner(owner_id, profile_id)
                .await
        }
    };
    if let Err(error) = validation {
        return trakt_error_response(error);
    }

    let history = match media.list_watch_history(profile_id).await {
        Ok(items) => items,
        Err(error) => return HttpResponse::InternalServerError().body(error.to_string()),
    };
    let ratings = match media.list_ratings(profile_id).await {
        Ok(items) => items,
        Err(error) => return HttpResponse::InternalServerError().body(error.to_string()),
    };
    let watchlist = match media.list_watchlist(profile_id).await {
        Ok(items) => items,
        Err(error) => return HttpResponse::InternalServerError().body(error.to_string()),
    };

    let mut seen = HashSet::new();
    let mut watched_movies = 0;
    let mut watched_shows = 0;
    let mut recently_watched = Vec::new();
    for event in history
        .iter()
        .filter(|event| event.source == media_tools::WatchHistorySource::Trakt)
    {
        if !seen.insert(event.media_item_id) {
            continue;
        }
        let item = match media.get_media_item(event.media_item_id).await {
            Ok(Some(item)) => item,
            Ok(None) => continue,
            Err(error) => return HttpResponse::InternalServerError().body(error.to_string()),
        };
        match item.kind {
            media_tools::MediaKind::Movie => watched_movies += 1,
            media_tools::MediaKind::TvShow => watched_shows += 1,
        }
        if recently_watched.len() < 5 {
            recently_watched.push(json!({
                "title": &item.title,
                "kind": match item.kind {
                    media_tools::MediaKind::Movie => "movie",
                    media_tools::MediaKind::TvShow => "tv_show",
                },
                "watched_at": &event.watched_at,
            }));
        }
    }

    let ratings_count = ratings
        .iter()
        .filter(|rating| rating.source == media_tools::WatchHistorySource::Trakt)
        .count();
    let watchlist_count = watchlist
        .iter()
        .filter(|item| item.source == media_tools::WatchHistorySource::Trakt)
        .count();

    HttpResponse::Ok().json(json!({
        "watched_movies": watched_movies,
        "watched_shows": watched_shows,
        "ratings": ratings_count,
        "watchlist_items": watchlist_count,
        "recently_watched": recently_watched,
    }))
}

#[post("/trakt/profiles/{profile_id}/sync")]
async fn sync_trakt_profile(
    request: HttpRequest,
    auth: Data<TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    state: Data<MediaApiState>,
    profile_id: PathParam<String>,
    query: Query<TraktSyncQuery>,
) -> impl Responder {
    let Some(sync) = state.trakt_sync.as_ref() else {
        return HttpResponse::ServiceUnavailable().body("Trakt synchronization is not configured");
    };
    let owner_id = match current_user_id(&request, &auth, &identities) {
        Ok(user_id) => user_id,
        Err(response) => return response,
    };
    let profile_id = match MediaProfileId::from_str(&profile_id) {
        Ok(id) => id,
        Err(error) => return HttpResponse::BadRequest().body(error.to_string()),
    };
    let Some(oauth) = state.trakt_oauth.as_ref() else {
        return HttpResponse::ServiceUnavailable().body("Trakt OAuth is not configured");
    };
    let validation = match query.family_id.as_deref() {
        Some(value) => match parse_id::<FamilyId>(value) {
            Ok(family_id) => {
                oauth
                    .validate_profile_owner(owner_id, family_id, profile_id)
                    .await
            }
            Err(error) => return HttpResponse::BadRequest().body(error),
        },
        None => {
            oauth
                .validate_personal_profile_owner(owner_id, profile_id)
                .await
        }
    };
    if let Err(error) = validation {
        return trakt_error_response(error);
    }
    match sync.sync_profile(owner_id, profile_id).await {
        Ok(summary) => HttpResponse::Ok().json(summary),
        Err(error) => trakt_sync_error_response(error),
    }
}

#[delete("/trakt/profiles/{profile_id}")]
async fn unlink_trakt_profile(
    request: HttpRequest,
    auth: Data<TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    state: Data<MediaApiState>,
    profile_id: PathParam<String>,
    body: Json<TraktProfileRequest>,
) -> impl Responder {
    let Some(oauth) = state.trakt_oauth.as_ref() else {
        return HttpResponse::ServiceUnavailable().body("Trakt OAuth is not configured");
    };
    let owner_id = match current_user_id(&request, &auth, &identities) {
        Ok(user_id) => user_id,
        Err(response) => return response,
    };
    let profile_id = match MediaProfileId::from_str(&profile_id) {
        Ok(id) => id,
        Err(error) => return HttpResponse::BadRequest().body(error.to_string()),
    };
    let result = match body.family_id.as_deref() {
        Some(value) => match parse_id::<FamilyId>(value) {
            Ok(family_id) => oauth.unlink_profile(owner_id, family_id, profile_id).await,
            Err(error) => return HttpResponse::BadRequest().body(error),
        },
        None => oauth.unlink_personal_profile(owner_id, profile_id).await,
    };
    match result {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(error) => trakt_error_response(error),
    }
}

/// Authenticated Trakt API scope. Mount below the platform's session middleware.
pub fn media_api() -> Scope {
    web::scope("/media")
        .service(list_trakt_profiles)
        .service(start_trakt_link)
        .service(complete_trakt_link)
        .service(trakt_profile_summary)
        .service(sync_trakt_profile)
        .service(unlink_trakt_profile)
}

fn current_user_id(
    request: &HttpRequest,
    auth: &TelegramAuth,
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

fn parse_id<T: for<'de> serde::Deserialize<'de>>(value: &str) -> Result<T, String> {
    serde_json::from_value(serde_json::Value::String(value.to_owned()))
        .map_err(|error| error.to_string())
}

fn trakt_error_response(error: TraktError) -> HttpResponse {
    match &error {
        TraktError::Forbidden => HttpResponse::Forbidden().body(error.to_string()),
        TraktError::ConnectionUnavailable => HttpResponse::NotFound().body(error.to_string()),
        TraktError::MediaStorage(
            media_tools::MediaStoreError::PendingNotFound
            | media_tools::MediaStoreError::PendingExpired,
        ) => HttpResponse::BadRequest().body(error.to_string()),
        _ => HttpResponse::BadRequest().body(error.to_string()),
    }
}

fn trakt_sync_error_response(error: TraktSyncError) -> HttpResponse {
    match &error {
        TraktSyncError::Forbidden => HttpResponse::Forbidden().body(error.to_string()),
        TraktSyncError::NotLinked => HttpResponse::NotFound().body(error.to_string()),
        TraktSyncError::RateLimited { .. } => {
            HttpResponse::TooManyRequests().body(error.to_string())
        }
        _ => HttpResponse::BadGateway().body(error.to_string()),
    }
}
