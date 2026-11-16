use std::{str::FromStr, sync::Arc, time::Duration};

use actix_web::{
    HttpRequest, HttpResponse, Responder, Scope, delete, get, post, put,
    web::{self, Data, Json, Path},
};
use agent_runtime::ToolContext;
use agent_service::{FamilyService, FamilyServiceError, RunManager, SubmitRunError};
use agent_storage::{
    Conversation, ConversationId, ConversationStore, ConversationStoreError, FamilyId,
    FileUserIdentityStore, UserIdentityStore,
};
use serde::{Deserialize, Serialize};
use tokio::time::sleep;

use super::types::{AgentCatalog, SubmitRunResponse};

fn authenticated_owner(
    request: &HttpRequest,
    auth: &crate::auth::TelegramAuth,
) -> Result<String, HttpResponse> {
    auth.owner_for_request(request)
        .ok_or_else(|| HttpResponse::Unauthorized().body("authentication required"))
}

#[derive(Debug, Deserialize)]
pub struct CreateConversationRequest {
    pub agent: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateConversationRequest {
    pub title: String,
}

#[derive(Debug, Deserialize)]
pub struct ContinueConversationRequest {
    pub input: String,
}

#[derive(Debug, Deserialize)]
pub struct SelectConversationFamilyRequest {
    pub family_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ConversationSummary {
    pub id: String,
    pub agent: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: usize,
    pub selected_family_id: Option<String>,
}

impl From<Conversation> for ConversationSummary {
    fn from(conversation: Conversation) -> Self {
        Self {
            id: conversation.id.to_string(),
            agent: conversation.agent,
            title: conversation.title,
            created_at: conversation.created_at.to_rfc3339(),
            updated_at: conversation.updated_at.to_rfc3339(),
            message_count: conversation.messages.len(),
            selected_family_id: conversation.selected_family_id.map(|id| id.to_string()),
        }
    }
}

fn conversation_error(error: ConversationStoreError) -> HttpResponse {
    match error {
        ConversationStoreError::NotFound(_) | ConversationStoreError::OwnerMismatch { .. } => {
            HttpResponse::NotFound().finish()
        }
        other => HttpResponse::InternalServerError().body(other.to_string()),
    }
}

#[get("")]
async fn list_conversations(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    store: Data<Arc<ConversationStore>>,
) -> impl Responder {
    let owner = match authenticated_owner(&request, &auth) {
        Ok(owner) => owner,
        Err(response) => return response,
    };
    match store.list(&owner) {
        Ok(conversations) => HttpResponse::Ok().json(
            conversations
                .into_iter()
                .map(ConversationSummary::from)
                .collect::<Vec<_>>(),
        ),
        Err(error) => conversation_error(error),
    }
}

#[post("")]
async fn create_conversation(
    request_http: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    store: Data<Arc<ConversationStore>>,
    catalog: Data<AgentCatalog>,
    request: Json<CreateConversationRequest>,
) -> impl Responder {
    let owner = match authenticated_owner(&request_http, &auth) {
        Ok(owner) => owner,
        Err(response) => return response,
    };
    let agent = request.agent.trim();
    if agent.is_empty() {
        return HttpResponse::BadRequest().body("agent must not be empty");
    }
    if !catalog.list().iter().any(|item| item.name == agent) {
        return HttpResponse::BadRequest().body("unknown agent");
    }

    match store.create(&owner, agent) {
        Ok(conversation) => HttpResponse::Created().json(ConversationSummary::from(conversation)),
        Err(error) => conversation_error(error),
    }
}

#[get("/{conversation_id}")]
async fn get_conversation(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    store: Data<Arc<ConversationStore>>,
    conversation_id: Path<String>,
) -> impl Responder {
    let owner = match authenticated_owner(&request, &auth) {
        Ok(owner) => owner,
        Err(response) => return response,
    };
    let id = match ConversationId::from_str(conversation_id.as_str()) {
        Ok(id) => id,
        Err(_) => {
            return HttpResponse::BadRequest().body("invalid conversation identifier");
        }
    };

    match store.get(id) {
        Ok(conversation) if conversation.owner == owner => HttpResponse::Ok().json(conversation),
        Ok(_) => HttpResponse::NotFound().finish(),
        Err(error) => conversation_error(error),
    }
}

#[put("/{conversation_id}")]
async fn rename_conversation(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    store: Data<Arc<ConversationStore>>,
    conversation_id: Path<String>,
    request_body: Json<UpdateConversationRequest>,
) -> impl Responder {
    let owner = match authenticated_owner(&request, &auth) {
        Ok(owner) => owner,
        Err(response) => return response,
    };
    let title = request_body.title.trim();
    if title.is_empty() {
        return HttpResponse::BadRequest().body("title must not be empty");
    }
    if title.chars().count() > 120 {
        return HttpResponse::BadRequest().body("title must be at most 120 characters");
    }

    let id = match ConversationId::from_str(conversation_id.as_str()) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().body("invalid conversation identifier"),
    };

    match store.rename(&owner, id, title) {
        Ok(conversation) => HttpResponse::Ok().json(ConversationSummary::from(conversation)),
        Err(error) => conversation_error(error),
    }
}

#[delete("/{conversation_id}")]
async fn delete_conversation(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    store: Data<Arc<ConversationStore>>,
    conversation_id: Path<String>,
) -> impl Responder {
    let owner = match authenticated_owner(&request, &auth) {
        Ok(owner) => owner,
        Err(response) => return response,
    };
    let id = match ConversationId::from_str(conversation_id.as_str()) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().body("invalid conversation identifier"),
    };

    match store.delete(&owner, id) {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(error) => conversation_error(error),
    }
}

#[put("/{conversation_id}/family")]
async fn select_conversation_family(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    store: Data<Arc<ConversationStore>>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<Arc<FamilyService>>,
    conversation_id: Path<String>,
    body: Json<SelectConversationFamilyRequest>,
) -> impl Responder {
    let owner = match authenticated_owner(&request, &auth) {
        Ok(owner) => owner,
        Err(response) => return response,
    };
    let id = match ConversationId::from_str(conversation_id.as_str()) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().body("invalid conversation identifier"),
    };
    match store.get(id) {
        Ok(conversation) if conversation.owner == owner => {}
        Ok(_) => return HttpResponse::NotFound().finish(),
        Err(error) => return conversation_error(error),
    }

    let family_id = match body.family_id.as_deref() {
        Some(value) => {
            let family_id = match FamilyId::from_str(value) {
                Ok(id) => id,
                Err(_) => return HttpResponse::BadRequest().body("invalid family identifier"),
            };
            let subject = match owner.strip_prefix("telegram:") {
                Some(subject) if !subject.is_empty() => subject,
                _ => return HttpResponse::Unauthorized().finish(),
            };
            let identity = match identities.find_identity("telegram", subject) {
                Ok(Some(identity)) => identity,
                Ok(None) => return HttpResponse::Unauthorized().finish(),
                Err(_) => return HttpResponse::InternalServerError().finish(),
            };
            match families.context_for_member(identity.user_id, family_id).await {
                Ok(_) => Some(family_id),
                Err(FamilyServiceError::NotMember | FamilyServiceError::NotFound) => {
                    return HttpResponse::NotFound().finish();
                }
                Err(_) => return HttpResponse::InternalServerError().finish(),
            }
        }
        None => None,
    };

    match store.set_selected_family(&owner, id, family_id) {
        Ok(conversation) => HttpResponse::Ok().json(ConversationSummary::from(conversation)),
        Err(error) => conversation_error(error),
    }
}

#[post("/{conversation_id}/runs")]
async fn continue_conversation(
    request_http: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    store: Data<Arc<ConversationStore>>,
    manager: Data<RunManager>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<Arc<FamilyService>>,
    conversation_id: Path<String>,
    request: Json<ContinueConversationRequest>,
) -> impl Responder {
    let owner = match authenticated_owner(&request_http, &auth) {
        Ok(owner) => owner,
        Err(response) => return response,
    };
    let input = request.input.trim();
    if input.is_empty() {
        return HttpResponse::BadRequest().body("input must not be empty");
    }

    let id = match ConversationId::from_str(conversation_id.as_str()) {
        Ok(id) => id,
        Err(_) => {
            return HttpResponse::BadRequest().body("invalid conversation identifier");
        }
    };

    let conversation = match store.get(id) {
        Ok(conversation) if conversation.owner == owner => conversation,
        Ok(_) => return HttpResponse::NotFound().finish(),
        Err(error) => return conversation_error(error),
    };

    let subject = match owner.strip_prefix("telegram:") {
        Some(subject) if !subject.is_empty() => subject,
        _ => return HttpResponse::Unauthorized().finish(),
    };
    let identity = match identities.find_identity("telegram", subject) {
        Ok(Some(identity)) => identity,
        Ok(None) => return HttpResponse::Unauthorized().finish(),
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };
    let family = match conversation.selected_family_id {
        Some(family_id) => match families.context_for_member(identity.user_id, family_id).await {
            Ok(context) => Some(context),
            Err(FamilyServiceError::NotMember | FamilyServiceError::NotFound) => {
                return HttpResponse::Conflict().body("selected family is no longer available");
            }
            Err(_) => return HttpResponse::InternalServerError().finish(),
        },
        None => None,
    };

    match manager
        .submit(
            conversation.agent.clone(),
            input.to_owned(),
            conversation.messages,
            ToolContext {
                user_id: Some(identity.user_id.to_string()),
                family,
                conversation_id: Some(id.to_string()),
                run_id: None,
            },
        )
        .await
    {
        Ok(run_id) => {
            auth.register_run(run_id.to_string(), &owner);
            let manager = manager.get_ref().clone();
            let store = store.get_ref().clone();
            tokio::spawn(async move {
                loop {
                    match manager.get(run_id).await {
                        Some(run) if run.status.is_terminal() => {
                            if let Some(messages) = manager.history(run_id).await {
                                if let Err(error) = store.save_messages(id, messages) {
                                    eprintln!("failed to persist conversation {id}: {error}");
                                }
                            }
                            break;
                        }
                        Some(_) => sleep(Duration::from_millis(250)).await,
                        None => break,
                    }
                }
            });

            HttpResponse::Accepted().json(SubmitRunResponse {
                run_id: run_id.to_string(),
                status: "queued",
            })
        }
        Err(SubmitRunError::QueueFull) => {
            HttpResponse::ServiceUnavailable().body("run queue is full")
        }
    }
}

pub fn conversations_api() -> Scope {
    web::scope("/conversations")
        .service(list_conversations)
        .service(create_conversation)
        .service(get_conversation)
        .service(rename_conversation)
        .service(delete_conversation)
        .service(continue_conversation)
}

#[cfg(test)]
mod tests {
    use std::{env, sync::Arc};

    use actix_web::{App, http::{header, StatusCode}, test, web};
    use agent_service::FamilyService;
    use agent_storage::{
        ConversationStore, FamilyRelationship, FileUserIdentityStore,
        InMemoryFamilyInvitationStore, InMemoryFamilyStore, UserIdentityStore,
    };
    use serde_json::{Value, json};
    use uuid::Uuid;

    use super::conversations_api;

    #[actix_web::test]
    async fn conversation_family_selection_requires_membership_and_can_be_cleared() {
        let directory = env::temp_dir().join(format!("eme-conversation-family-{}", Uuid::new_v4()));
        let store = Arc::new(ConversationStore::open(directory.join("conversations")).unwrap());
        let identities = Arc::new(FileUserIdentityStore::open(directory.join("users.json")).unwrap());
        let (user, _) = identities.resolve_or_create("telegram", "42", None).unwrap();
        let conversation = store.create("telegram:42", "assistant").unwrap();

        let families = Arc::new(InMemoryFamilyStore::default());
        let invitations = Arc::new(InMemoryFamilyInvitationStore::new(families.clone()));
        let service = Arc::new(FamilyService::new(families, invitations));
        let family = service
            .create_family(user.id, "Home", FamilyRelationship::Father)
            .await
            .unwrap();
        let outsider = identities.resolve_or_create("telegram", "84", None).unwrap().0;
        let outsider_family = service
            .create_family(outsider.id, "Other home", FamilyRelationship::Other)
            .await
            .unwrap();

        let auth = Arc::new(crate::auth::TelegramAuth::new(None, false));
        let app = test::init_service(
            App::new()
                .app_data(web::Data::from(Arc::clone(&auth)))
                .app_data(web::Data::new(Arc::clone(&store)))
                .app_data(web::Data::new(Arc::clone(&identities)))
                .app_data(web::Data::new(Arc::clone(&service)))
                .service(crate::auth::auth_api())
                .service(conversations_api()),
        )
        .await;

        let response = test::call_service(
            &app,
            test::TestRequest::post().uri("/auth/telegram/start").to_request(),
        )
        .await;
        let login: Value = test::read_body_json(response).await;
        let token = login["token"].as_str().unwrap();
        assert!(auth.complete_login(token, 42, identities.as_ref()));
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/auth/telegram/status")
                .set_json(json!({"token": token}))
                .to_request(),
        )
        .await;
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

        let response = test::call_service(
            &app,
            test::TestRequest::put()
                .uri(&format!("/conversations/{}/family", conversation.id))
                .insert_header((header::COOKIE, cookie.clone()))
                .set_json(json!({"family_id": outsider_family.id.to_string()}))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = test::call_service(
            &app,
            test::TestRequest::put()
                .uri(&format!("/conversations/{}/family", conversation.id))
                .insert_header((header::COOKIE, cookie.clone()))
                .set_json(json!({"family_id": family.id.to_string()}))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let selected: Value = test::read_body_json(response).await;
        assert_eq!(selected["selected_family_id"], family.id.to_string());

        let response = test::call_service(
            &app,
            test::TestRequest::put()
                .uri(&format!("/conversations/{}/family", conversation.id))
                .insert_header((header::COOKIE, cookie))
                .set_json(json!({"family_id": null}))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let cleared: Value = test::read_body_json(response).await;
        assert!(cleared["selected_family_id"].is_null());

        drop(app);
        drop(store);
        drop(identities);
        let _ = std::fs::remove_dir_all(directory);
    }
}
