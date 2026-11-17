//! HTTP endpoints for family membership, roles, and invitations.

mod accept_invitation_request;
mod change_relationship_request;
mod change_role_request;
mod create_family_request;
mod create_invitation_request;
mod created_invitation;
mod family_details;
mod lan_invitation_config;
mod rename_family_request;
mod transfer_ownership_request;

pub use accept_invitation_request::AcceptInvitationRequest;
pub use change_relationship_request::ChangeRelationshipRequest;
pub use change_role_request::ChangeRoleRequest;
pub use create_family_request::CreateFamilyRequest;
pub use create_invitation_request::CreateInvitationRequest;
pub use created_invitation::CreatedInvitation;
pub use family_details::{FamilyDetails, FamilyMemberDetails};
pub use lan_invitation_config::LanInvitationConfig;
pub use rename_family_request::RenameFamilyRequest;
pub use transfer_ownership_request::TransferOwnershipRequest;

use std::{net::IpAddr, str::FromStr, sync::Arc};

use actix_web::{
    HttpRequest, HttpResponse, Responder, Scope, delete, get, post, put,
    web::{self, Data, Json, Path},
};
use agent_service::{FamilyService, FamilyServiceError};
use agent_storage::{
    FamilyId, FileUserIdentityStore, InvitationTargetIdentity, InvitationToken, UserId,
    UserIdentityStore,
};

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

/// Explicit LAN base URL used to build invitation links. Only private, loopback,
/// or link-local IP addresses are accepted; Host headers are never trusted.

impl LanInvitationConfig {
    pub fn parse(value: &str) -> Result<Self, String> {
        let value = value.trim().trim_end_matches('/');
        let authority = value
            .strip_prefix("http://")
            .ok_or_else(|| "LAN_BASE_URL must use plain http:// on a private network".to_owned())?;
        if authority.is_empty()
            || authority.contains('/')
            || authority.contains('?')
            || authority.contains('#')
            || authority.contains('@')
        {
            return Err("LAN_BASE_URL must contain only an IP address and optional port".into());
        }
        let host = if authority.starts_with('[') {
            let (host, suffix) = authority
                .split_once(']')
                .ok_or_else(|| "LAN_BASE_URL has an invalid IPv6 address".to_owned())?;
            if !suffix.is_empty()
                && (!suffix.starts_with(':') || suffix[1..].parse::<u16>().is_err())
            {
                return Err("LAN_BASE_URL has an invalid port".into());
            }
            &host[1..]
        } else if let Ok(ip) = authority.parse::<IpAddr>() {
            if ip.is_ipv6() {
                return Err("IPv6 LAN_BASE_URL addresses must be enclosed in brackets".into());
            }
            authority
        } else {
            let (host, port) = authority
                .split_once(':')
                .ok_or_else(|| "LAN_BASE_URL must include an IP address".to_owned())?;
            if host.contains(':') || port.parse::<u16>().is_err() {
                return Err("LAN_BASE_URL has an invalid address or port".into());
            }
            host
        };
        let ip: IpAddr = host.parse().map_err(|_| {
            "LAN_BASE_URL host must be a private or link-local IP address".to_owned()
        })?;
        let allowed = match ip {
            IpAddr::V4(ip) => ip.is_private() || ip.is_loopback() || ip.is_link_local(),
            IpAddr::V6(ip) => {
                ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local()
            }
        };
        if !allowed || ip.is_unspecified() {
            return Err(
                "LAN_BASE_URL must use a private, loopback, or link-local IP address".into(),
            );
        }
        Ok(Self {
            base_url: value.to_owned(),
        })
    }

    pub fn from_env() -> Result<Option<Self>, String> {
        match std::env::var("LAN_BASE_URL") {
            Ok(value) => Self::parse(&value).map(Some),
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(error) => Err(format!("reading LAN_BASE_URL: {error}")),
        }
    }

    fn invitation_url(&self, token: &str) -> String {
        format!("{}/invite/{}", self.base_url, token)
    }
}

#[get("/current-user")]
async fn current_family_user(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
) -> impl Responder {
    match authenticated_actor(&request, &auth, &identities) {
        Ok((user_id, _)) => HttpResponse::Ok().json(serde_json::json!({ "user_id": user_id })),
        Err(response) => response,
    }
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
        Ok((family, members)) => {
            let members = match members
                .into_iter()
                .map(|member| {
                    identities
                        .get_user(member.user_id.clone())
                        .map(|user| FamilyMemberDetails {
                            member,
                            display_name: user.and_then(|user| user.display_name),
                        })
                })
                .collect::<Result<Vec<_>, _>>()
            {
                Ok(members) => members,
                Err(_) => return HttpResponse::InternalServerError().finish(),
            };
            HttpResponse::Ok().json(FamilyDetails { family, members })
        }
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
    lan: Option<Data<LanInvitationConfig>>,
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
            lan_url: lan.map(|config| config.invitation_url(token.expose())),
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
        .service(current_family_user)
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

    use super::{LanInvitationConfig, families_api, invitations_api};

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
                .app_data(web::Data::new(
                    LanInvitationConfig::parse("http://192.168.1.20:8080").unwrap(),
                ))
                .service(crate::auth::auth_api())
                .service(families_api())
                .service(invitations_api()),
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
        assert!(auth.complete_login(login_token, 42, None, identities.as_ref()));

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
                .insert_header((header::COOKIE, cookie.clone()))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let listed: Value = test::read_body_json(response).await;
        assert_eq!(listed.as_array().unwrap().len(), 1);

        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/families/current-user")
                .insert_header((header::COOKIE, cookie.clone()))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let current_user: Value = test::read_body_json(response).await;
        assert_eq!(current_user["user_id"], identity.user_id.to_string());

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
        assert_eq!(
            created_invitation["lan_url"],
            format!("http://192.168.1.20:8080/invite/{invitation_token}")
        );

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

#[cfg(test)]
mod lan_url_tests {
    use super::LanInvitationConfig;

    #[test]
    fn accepts_only_private_network_addresses_and_builds_invitation_urls() {
        let config = LanInvitationConfig::parse("http://192.168.1.20:8080/").unwrap();
        assert_eq!(
            config.invitation_url("secret"),
            "http://192.168.1.20:8080/invite/secret"
        );
        assert!(LanInvitationConfig::parse("https://192.168.1.20").is_err());
        assert!(LanInvitationConfig::parse("http://example.com:8080").is_err());
        assert!(LanInvitationConfig::parse("http://8.8.8.8:8080").is_err());
        assert!(LanInvitationConfig::parse("http://192.168.1.20:bad").is_err());
        assert!(LanInvitationConfig::parse("http://192.168.1.20/path").is_err());
        assert!(LanInvitationConfig::parse("http://::1").is_err());
        assert!(LanInvitationConfig::parse("http://[fd00::20]:8080").is_ok());
    }
}
