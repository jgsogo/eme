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
    display_name: String,
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
    match identities.update_display_name(user.id, &body.display_name) {
        Ok(user) => HttpResponse::Ok().json(user),
        Err(IdentityStoreError::InvalidDisplayName) => {
            HttpResponse::BadRequest().body("display name must contain between 1 and 80 characters")
        }
        Err(IdentityStoreError::UserNotFound(_)) => HttpResponse::NotFound().finish(),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

pub fn profile_api() -> Scope {
    web::scope("/profile")
        .service(get_profile)
        .service(update_profile)
}
