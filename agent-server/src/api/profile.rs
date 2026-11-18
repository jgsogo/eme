//! Authenticated endpoints for the current user's profile.

use std::sync::Arc;

use actix_web::{
    HttpRequest, HttpResponse, Responder, Scope, get, put,
    web::{self, Data, Json},
};
use agent_storage::{FileUserIdentityStore, IdentityStoreError, UserIdentityStore};
use serde::Deserialize;

#[derive(Deserialize)]
struct UpdateProfileRequest {
    display_name: Option<String>,
    timezone: Option<String>,
}

fn current_user(
    request: &HttpRequest,
    auth: &crate::auth::TelegramAuth,
    identities: &FileUserIdentityStore,
) -> Result<agent_storage::User, HttpResponse> {
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
    identities
        .get_user(identity.user_id)
        .map_err(|_| HttpResponse::InternalServerError().finish())?
        .ok_or_else(|| HttpResponse::Unauthorized().finish())
}

#[get("")]
async fn get_profile(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
) -> impl Responder {
    match current_user(&request, &auth, &identities) {
        Ok(user) => HttpResponse::Ok().json(user),
        Err(response) => response,
    }
}

#[put("")]
async fn update_profile(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    body: Json<UpdateProfileRequest>,
) -> impl Responder {
    let user = match current_user(&request, &auth, &identities) {
        Ok(user) => user,
        Err(response) => return response,
    };

    if body.display_name.is_none() && body.timezone.is_none() {
        return HttpResponse::BadRequest().body("at least one profile field must be provided");
    }

    let mut updated = user;
    if let Some(display_name) = body.display_name.as_deref() {
        updated = match identities.update_display_name(updated.id, display_name) {
            Ok(user) => user,
            Err(IdentityStoreError::InvalidDisplayName) => {
                return HttpResponse::BadRequest()
                    .body("display name must contain between 1 and 80 characters");
            }
            Err(IdentityStoreError::UserNotFound(_)) => return HttpResponse::NotFound().finish(),
            Err(_) => return HttpResponse::InternalServerError().finish(),
        };
    }
    if let Some(timezone) = body.timezone.as_deref() {
        updated = match identities.update_timezone(updated.id, Some(timezone)) {
            Ok(user) => user,
            Err(IdentityStoreError::InvalidTimezone) => {
                return HttpResponse::BadRequest()
                    .body("timezone must be an IANA timezone identifier, e.g. Europe/Madrid");
            }
            Err(IdentityStoreError::UserNotFound(_)) => return HttpResponse::NotFound().finish(),
            Err(_) => return HttpResponse::InternalServerError().finish(),
        };
    }
    HttpResponse::Ok().json(updated)
}

pub fn profile_api() -> Scope {
    web::scope("/profile")
        .service(get_profile)
        .service(update_profile)
}
