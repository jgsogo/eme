use std::{str::FromStr, sync::Arc};

use actix_web::{
    HttpRequest, HttpResponse, Responder, Scope, delete, get, post, put,
    web::{self, Data, Json, Path},
};
use agent_service::{FamilyService, FamilyServiceError};
use agent_storage::{
    Family, FamilyId, FamilyInvitationSummary, FamilyMember, FamilyRelationship, FamilyRole,
    FileUserIdentityStore, InvitationTargetIdentity, InvitationToken, UserId, UserIdentityStore,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

fn authenticated_actor(
    request: &HttpRequest,
    auth: &crate::auth::TelegramAuth,
    identities: &FileUserIdentityStore,
) -> Result<(UserId, InvitationTargetIdentity), HttpResponse> {
    let owner = auth
        .owner_for_request(request)
        .ok_or_else(|| HttpResponse::Unauthorized().finish())?;
    let subject = owner
        .strip_prefix("telegram:")
        .filter(|subject| !subject.is_empty())
        .ok_or_else(|| HttpResponse::Unauthorized().finish())?;
    let identity = identities
        .find_identity("telegram", subject)
        .map_err(|_| HttpResponse::InternalServerError().finish())?
        .ok_or_else(|| HttpResponse::Unauthorized().finish())?;
    let external = InvitationTargetIdentity::new("telegram", subject)
        .map_err(|_| HttpResponse::Unauthorized().finish())?;
    Ok((identity.user_id, external))
}

fn service_error(error: FamilyServiceError) -> HttpResponse {
    match error {
        FamilyServiceError::NotFound | FamilyServiceError::NotMember => {
            HttpResponse::NotFound().finish()
        }
        FamilyServiceError::Forbidden => HttpResponse::Forbidden().finish(),
        FamilyServiceError::EmptyName => {
            HttpResponse::BadRequest().body("family name must not be empty")
        }
        FamilyServiceError::OwnerMustTransfer
        | FamilyServiceError::TargetNotMember
        | FamilyServiceError::OwnerRoleRequiresTransfer
        | FamilyServiceError::CannotChangeOwnRole => {
            HttpResponse::Conflict().body("family membership conflict")
        }
        FamilyServiceError::FamilyStorage(_) | FamilyServiceError::InvitationStorage(_) => {
            HttpResponse::InternalServerError().finish()
        }
    }
}

fn parse_family_id(value: &str) -> Result<FamilyId, HttpResponse> {
    FamilyId::from_str(value)
        .map_err(|_| HttpResponse::BadRequest().body("invalid family identifier"))
}

fn parse_user_id(value: &str) -> Result<UserId, HttpResponse> {
    UserId::from_str(value).map_err(|_| HttpResponse::BadRequest().body("invalid user identifier"))
}

#[derive(Debug, Deserialize)]
pub struct CreateFamilyRequest {
    pub name: String,
    pub relationship: FamilyRelationship,
}

#[derive(Debug, Deserialize)]
pub struct RenameFamilyRequest {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateInvitationRequest {
    pub expires_at: DateTime<Utc>,
    #[serde(default)]
    pub target_identity: Option<InvitationTargetIdentity>,
}

#[derive(Debug, Deserialize)]
pub struct ChangeRoleRequest {
    pub role: FamilyRole,
}

#[derive(Debug, Deserialize)]
pub struct ChangeRelationshipRequest {
    pub relationship: FamilyRelationship,
}

#[derive(Debug, Deserialize)]
pub struct TransferOwnershipRequest {
    pub new_owner_user_id: String,
}

#[derive(Debug, Deserialize)]
pub struct AcceptInvitationRequest {
    pub token: String,
    pub relationship: FamilyRelationship,
}

#[derive(Debug, Serialize)]
pub struct FamilyDetails {
    pub family: Family,
    pub members: Vec<FamilyMember>,
}

#[derive(Debug, Serialize)]
pub struct CreatedInvitation {
    pub invitation: FamilyInvitationSummary,
    /// Returned only by the creation operation; never included in list responses.
    pub token: String,
}

#[get("")]
async fn list_families(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    service: Data<Arc<FamilyService>>,
) -> impl Responder {
    let (actor, _) = match authenticated_actor(&request, &auth, &identities) {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    match service.list_families(actor).await {
        Ok(families) => HttpResponse::Ok().json(families),
        Err(error) => service_error(error),
    }
}

#[post("")]
async fn create_family(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    service: Data<Arc<FamilyService>>,
    body: Json<CreateFamilyRequest>,
) -> impl Responder {
    let (actor, _) = match authenticated_actor(&request, &auth, &identities) {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    match service
        .create_family(actor, &body.name, body.relationship)
        .await
    {
        Ok(family) => HttpResponse::Created().json(family),
        Err(error) => service_error(error),
    }
}

#[get("/{family_id}")]
async fn get_family(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    service: Data<Arc<FamilyService>>,
    family_id: Path<String>,
) -> impl Responder {
    let (actor, _) = match authenticated_actor(&request, &auth, &identities) {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    let family_id = match parse_family_id(&family_id) {
        Ok(id) => id,
        Err(response) => return response,
    };
    match service.get_family(actor, family_id).await {
        Ok((family, members)) => HttpResponse::Ok().json(FamilyDetails { family, members }),
        Err(error) => service_error(error),
    }
}

#[put("/{family_id}")]
async fn rename_family(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    service: Data<Arc<FamilyService>>,
    family_id: Path<String>,
    body: Json<RenameFamilyRequest>,
) -> impl Responder {
    let (actor, _) = match authenticated_actor(&request, &auth, &identities) {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    let family_id = match parse_family_id(&family_id) {
        Ok(id) => id,
        Err(response) => return response,
    };
    match service.rename_family(actor, family_id, &body.name).await {
        Ok(family) => HttpResponse::Ok().json(family),
        Err(error) => service_error(error),
    }
}

#[delete("/{family_id}")]
async fn delete_family(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    service: Data<Arc<FamilyService>>,
    family_id: Path<String>,
) -> impl Responder {
    let (actor, _) = match authenticated_actor(&request, &auth, &identities) {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    let family_id = match parse_family_id(&family_id) {
        Ok(id) => id,
        Err(response) => return response,
    };
    match service.delete_family(actor, family_id).await {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(error) => service_error(error),
    }
}

#[get("/{family_id}/members")]
async fn list_members(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    service: Data<Arc<FamilyService>>,
    family_id: Path<String>,
) -> impl Responder {
    let (actor, _) = match authenticated_actor(&request, &auth, &identities) {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    let family_id = match parse_family_id(&family_id) {
        Ok(id) => id,
        Err(response) => return response,
    };
    match service.list_members(actor, family_id).await {
        Ok(members) => HttpResponse::Ok().json(members),
        Err(error) => service_error(error),
    }
}

#[delete("/{family_id}/members/{user_id}")]
async fn remove_member(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    service: Data<Arc<FamilyService>>,
    path: Path<(String, String)>,
) -> impl Responder {
    let (actor, _) = match authenticated_actor(&request, &auth, &identities) {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    let (family_id, user_id) = path.into_inner();
    let family_id = match parse_family_id(&family_id) {
        Ok(id) => id,
        Err(response) => return response,
    };
    let user_id = match parse_user_id(&user_id) {
        Ok(id) => id,
        Err(response) => return response,
    };
    match service.remove_member(actor, family_id, user_id).await {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(error) => service_error(error),
    }
}

#[put("/{family_id}/members/{user_id}/role")]
async fn change_member_role(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    service: Data<Arc<FamilyService>>,
    path: Path<(String, String)>,
    body: Json<ChangeRoleRequest>,
) -> impl Responder {
    let (actor, _) = match authenticated_actor(&request, &auth, &identities) {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    let (family_id, user_id) = path.into_inner();
    let family_id = match parse_family_id(&family_id) {
        Ok(id) => id,
        Err(response) => return response,
    };
    let user_id = match parse_user_id(&user_id) {
        Ok(id) => id,
        Err(response) => return response,
    };
    match service
        .change_member_role(actor, family_id, user_id, body.role)
        .await
    {
        Ok(member) => HttpResponse::Ok().json(member),
        Err(error) => service_error(error),
    }
}

#[put("/{family_id}/members/{user_id}/relationship")]
async fn change_member_relationship(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    service: Data<Arc<FamilyService>>,
    path: Path<(String, String)>,
    body: Json<ChangeRelationshipRequest>,
) -> impl Responder {
    let (actor, _) = match authenticated_actor(&request, &auth, &identities) {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    let (family_id, user_id) = path.into_inner();
    let family_id = match parse_family_id(&family_id) {
        Ok(id) => id,
        Err(response) => return response,
    };
    let user_id = match parse_user_id(&user_id) {
        Ok(id) => id,
        Err(response) => return response,
    };
    match service
        .change_member_relationship(actor, family_id, user_id, body.relationship)
        .await
    {
        Ok(member) => HttpResponse::Ok().json(member),
        Err(error) => service_error(error),
    }
}

#[post("/{family_id}/transfer-ownership")]
async fn transfer_ownership(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    service: Data<Arc<FamilyService>>,
    family_id: Path<String>,
    body: Json<TransferOwnershipRequest>,
) -> impl Responder {
    let (actor, _) = match authenticated_actor(&request, &auth, &identities) {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    let family_id = match parse_family_id(&family_id) {
        Ok(id) => id,
        Err(response) => return response,
    };
    let new_owner = match parse_user_id(&body.new_owner_user_id) {
        Ok(id) => id,
        Err(response) => return response,
    };
    match service
        .transfer_ownership(actor, family_id, new_owner)
        .await
    {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(error) => service_error(error),
    }
}

#[post("/{family_id}/leave")]
async fn leave_family(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    service: Data<Arc<FamilyService>>,
    family_id: Path<String>,
) -> impl Responder {
    let (actor, _) = match authenticated_actor(&request, &auth, &identities) {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    let family_id = match parse_family_id(&family_id) {
        Ok(id) => id,
        Err(response) => return response,
    };
    match service.leave_family(actor, family_id).await {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(error) => service_error(error),
    }
}

#[post("/{family_id}/invitations")]
async fn create_invitation(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    service: Data<Arc<FamilyService>>,
    family_id: Path<String>,
    body: Json<CreateInvitationRequest>,
) -> impl Responder {
    let (actor, _) = match authenticated_actor(&request, &auth, &identities) {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    let family_id = match parse_family_id(&family_id) {
        Ok(id) => id,
        Err(response) => return response,
    };
    match service
        .create_invitation(
            actor,
            family_id,
            body.target_identity.clone(),
            body.expires_at,
        )
        .await
    {
        Ok((invitation, token)) => HttpResponse::Created().json(CreatedInvitation {
            invitation,
            token: token.expose().to_owned(),
        }),
        Err(error) => service_error(error),
    }
}

#[get("/{family_id}/invitations")]
async fn list_invitations(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    service: Data<Arc<FamilyService>>,
    family_id: Path<String>,
) -> impl Responder {
    let (actor, _) = match authenticated_actor(&request, &auth, &identities) {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    let family_id = match parse_family_id(&family_id) {
        Ok(id) => id,
        Err(response) => return response,
    };
    match service.list_invitations(actor, family_id).await {
        Ok(invitations) => HttpResponse::Ok().json(invitations),
        Err(error) => service_error(error),
    }
}

#[delete("/{family_id}/invitations/{invitation_id}")]
async fn revoke_invitation(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    service: Data<Arc<FamilyService>>,
    path: Path<(String, String)>,
) -> impl Responder {
    let (actor, _) = match authenticated_actor(&request, &auth, &identities) {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    let (family_id, invitation_id) = path.into_inner();
    let family_id = match parse_family_id(&family_id) {
        Ok(id) => id,
        Err(response) => return response,
    };
    let invitation_id = match Uuid::parse_str(&invitation_id) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().body("invalid invitation identifier"),
    };
    match service
        .revoke_invitation(actor, family_id, invitation_id)
        .await
    {
        Ok(invitation) => HttpResponse::Ok().json(invitation),
        Err(error) => service_error(error),
    }
}

#[post("/accept")]
async fn accept_invitation(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    service: Data<Arc<FamilyService>>,
    body: Json<AcceptInvitationRequest>,
) -> impl Responder {
    let (actor, external_identity) = match authenticated_actor(&request, &auth, &identities) {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    let token = match InvitationToken::parse(&body.token) {
        Ok(token) => token,
        Err(_) => return HttpResponse::NotFound().finish(),
    };
    match service
        .accept_invitation(&token, &external_identity, actor, body.relationship)
        .await
    {
        Ok(member) => HttpResponse::Ok().json(member),
        Err(error) => service_error(error),
    }
}

pub fn families_api() -> Scope {
    web::scope("/families")
        .service(list_families)
        .service(create_family)
        .service(get_family)
        .service(rename_family)
        .service(delete_family)
        .service(list_members)
        .service(remove_member)
        .service(change_member_role)
        .service(change_member_relationship)
        .service(transfer_ownership)
        .service(leave_family)
        .service(create_invitation)
        .service(list_invitations)
        .service(revoke_invitation)
}

pub fn invitations_api() -> Scope {
    web::scope("/invitations").service(accept_invitation)
}

#[cfg(test)]
mod tests {
    use std::{env, sync::Arc};

    use actix_web::{
        App,
        http::{StatusCode, header},
        test, web,
    };
    use agent_service::FamilyService;
    use agent_storage::{
        FileUserIdentityStore, InMemoryFamilyInvitationStore, InMemoryFamilyStore,
        UserIdentityStore,
    };
    use serde_json::{Value, json};

    use super::families_api;

    #[actix_web::test]
    async fn family_api_requires_authentication_and_uses_session_identity_as_actor() {
        let identity_path = env::temp_dir().join(format!(
            "eme-family-api-identities-{}.json",
            uuid::Uuid::new_v4()
        ));
        let identities = Arc::new(FileUserIdentityStore::open(&identity_path).unwrap());
        let auth = Arc::new(crate::auth::TelegramAuth::new(None, false));
        let families = Arc::new(InMemoryFamilyStore::default());
        let invitations = Arc::new(InMemoryFamilyInvitationStore::new(families.clone()));
        let service = Arc::new(FamilyService::new(families, invitations));

        let app = test::init_service(
            App::new()
                .app_data(web::Data::from(Arc::clone(&auth)))
                .app_data(web::Data::new(Arc::clone(&identities)))
                .app_data(web::Data::new(Arc::clone(&service)))
                .service(crate::auth::auth_api())
                .service(families_api()),
        )
        .await;

        let response =
            test::call_service(&app, test::TestRequest::get().uri("/families").to_request()).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/auth/telegram/start")
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let login: Value = test::read_body_json(response).await;
        let login_token = login["token"].as_str().unwrap();
        assert!(auth.complete_login(login_token, 42, identities.as_ref()));

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/auth/telegram/status")
                .set_json(json!({"token": login_token}))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();

        // A caller cannot choose the actor or creator in the request body.
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/families")
                .insert_header((header::COOKIE, cookie.clone()))
                .set_json(json!({
                    "name": "Home",
                    "relationship": "other",
                    "creator_id": "00000000-0000-0000-0000-000000000000",
                    "user_id": "00000000-0000-0000-0000-000000000000"
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let family: Value = test::read_body_json(response).await;

        let identity = identities.find_identity("telegram", "42").unwrap().unwrap();
        assert_eq!(family["creator_id"], identity.user_id.to_string());

        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/families")
                .insert_header((header::COOKIE, cookie))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let listed: Value = test::read_body_json(response).await;
        assert_eq!(listed.as_array().unwrap().len(), 1);

        let family_id = family["id"].as_str().unwrap();
        let expires_at = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(&format!("/families/{family_id}/invitations"))
                .insert_header((header::COOKIE, cookie.clone()))
                .set_json(json!({"expires_at": expires_at}))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let created_invitation: Value = test::read_body_json(response).await;
        let invitation_token = created_invitation["token"].as_str().unwrap().to_owned();

        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/families/{family_id}/invitations"))
                .insert_header((header::COOKIE, cookie.clone()))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let invitation_list: Value = test::read_body_json(response).await;
        assert_eq!(invitation_list.as_array().unwrap().len(), 1);
        assert!(!invitation_list.to_string().contains(&invitation_token));

        // A second authenticated Telegram identity can accept the bearer token.
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/auth/telegram/start")
                .to_request(),
        )
        .await;
        let second_login: Value = test::read_body_json(response).await;
        let second_login_token = second_login["token"].as_str().unwrap();
        assert!(auth.complete_login(second_login_token, 84, identities.as_ref()));

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/auth/telegram/status")
                .set_json(json!({"token": second_login_token}))
                .to_request(),
        )
        .await;
        let second_cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/invitations/accept")
                .insert_header((header::COOKIE, second_cookie.clone()))
                .set_json(json!({
                    "token": invitation_token,
                    "relationship": "son"
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);

        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/families/{family_id}"))
                .insert_header((header::COOKIE, second_cookie))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let details: Value = test::read_body_json(response).await;
        assert_eq!(details["members"].as_array().unwrap().len(), 2);

        drop(app);
        drop(service);
        drop(identities);
        let _ = std::fs::remove_file(identity_path);
    }
}
