//! HTTP endpoints and request handling for conversation lifecycle operations.

mod continue_conversation_request;
mod conversation_summary;
mod create_conversation_request;
mod select_conversation_family_request;
mod update_conversation_request;

pub use continue_conversation_request::ContinueConversationRequest;
pub use conversation_summary::ConversationSummary;
pub use create_conversation_request::CreateConversationRequest;
use select_conversation_family_request::SelectConversationFamilyRequest;
pub use update_conversation_request::UpdateConversationRequest;

use std::{str::FromStr, sync::Arc};

use actix_web::{
    HttpRequest, HttpResponse, Responder, Scope, delete, get, post, put,
    web::{self, Data, Json, Path},
};
use agent_runtime::{Message, ToolContext};
use agent_service::{FamilyService, FamilyServiceError, RunManager, RunStatus, SubmitRunError};
use agent_storage::{
    Conversation, ConversationId, SqliteConversationStore, ConversationStoreError, FamilyId,
    FileUserIdentityStore, UserId, UserIdentityStore,
};

use super::types::{AgentCatalog, GetRunResponse, SubmitRunResponse};

fn conversation_history_for_submission(
    mut history: Vec<Message>,
    input: &str,
    retry: bool,
) -> Option<Vec<Message>> {
    if !retry {
        return Some(history);
    }

    let message_index = history.iter().rposition(
        |message| matches!(message, Message::User { content } if content.as_str() == input),
    )?;
    history.truncate(message_index);
    Some(history)
}

fn authenticated_owner(
    request: &HttpRequest,
    auth: &crate::auth::TelegramAuth,
) -> Result<String, HttpResponse> {
    auth.owner_for_request(request)
        .ok_or_else(|| HttpResponse::Unauthorized().body("authentication required"))
}

impl From<Conversation> for ConversationSummary {
    fn from(conversation: Conversation) -> Self {
        Self {
            id: conversation.id.to_string(),
            agent: conversation.agent,
            title: conversation.title,
            archived: conversation.archived,
            created_at: conversation.created_at.to_rfc3339(),
            updated_at: conversation.updated_at.to_rfc3339(),
            message_count: conversation.messages.len(),
            selected_family_id: conversation.selected_family_id.map(|id| id.to_string()),
            history_tokens_estimate: estimate_history_tokens(&conversation.messages),
        }
    }
}

#[derive(serde::Serialize)]
struct ConversationDetail {
    #[serde(flatten)]
    conversation: Conversation,
    history_tokens_estimate: usize,
}

fn estimate_history_tokens(messages: &[agent_runtime::Message]) -> usize {
    serde_json::to_string(messages)
        .map(|serialized| serialized.len().div_ceil(4))
        .unwrap_or_default()
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
    store: Data<Arc<SqliteConversationStore>>,
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

#[get("/archived")]
async fn list_archived_conversations(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    store: Data<Arc<ConversationStore>>,
) -> impl Responder {
    let owner = match authenticated_owner(&request, &auth) {
        Ok(owner) => owner,
        Err(response) => return response,
    };
    match store.list_archived(&owner) {
        Ok(conversations) => HttpResponse::Ok().json(
            conversations
                .into_iter()
                .map(ConversationSummary::from)
                .collect::<Vec<_>>(),
        ),
        Err(error) => conversation_error(error),
    }
}

#[put("/{conversation_id}/archive")]
async fn archive_conversation(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    store: Data<Arc<ConversationStore>>,
    conversation_id: Path<String>,
) -> impl Responder {
    set_conversation_archived(request, auth, store, conversation_id, true)
}

#[put("/{conversation_id}/unarchive")]
async fn unarchive_conversation(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    store: Data<Arc<ConversationStore>>,
    conversation_id: Path<String>,
) -> impl Responder {
    set_conversation_archived(request, auth, store, conversation_id, false)
}

fn set_conversation_archived(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    store: Data<Arc<ConversationStore>>,
    conversation_id: Path<String>,
    archived: bool,
) -> HttpResponse {
    let owner = match authenticated_owner(&request, &auth) {
        Ok(owner) => owner,
        Err(response) => return response,
    };
    let id = match ConversationId::from_str(conversation_id.as_str()) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().body("invalid conversation identifier"),
    };

    match store.set_archived(&owner, id, archived) {
        Ok(conversation) => HttpResponse::Ok().json(ConversationSummary::from(conversation)),
        Err(error) => conversation_error(error),
    }
}

#[post("")]
async fn create_conversation(
    request_http: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    store: Data<Arc<SqliteConversationStore>>,
    catalog: Data<AgentCatalog>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<Arc<FamilyService>>,
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

    let family_id = match request.family_id.as_deref() {
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
            match families
                .context_for_member(identity.user_id, family_id)
                .await
            {
                Ok(_) => Some(family_id),
                Err(FamilyServiceError::NotMember | FamilyServiceError::NotFound) => {
                    return HttpResponse::NotFound().finish();
                }
                Err(_) => return HttpResponse::InternalServerError().finish(),
            }
        }
        None => None,
    };

    match store.create(&owner, agent) {
        Ok(conversation) => {
            let conversation = match family_id {
                Some(family_id) => {
                    match store.set_selected_family(&owner, conversation.id, Some(family_id)) {
                        Ok(conversation) => conversation,
                        Err(error) => return conversation_error(error),
                    }
                }
                None => conversation,
            };
            HttpResponse::Created().json(ConversationSummary::from(conversation))
        }
        Err(error) => conversation_error(error),
    }
}

#[get("/{conversation_id}")]
async fn get_conversation(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    store: Data<Arc<SqliteConversationStore>>,
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
        Ok(conversation) if conversation.owner == owner => {
            let history_tokens_estimate = estimate_history_tokens(&conversation.messages);
            HttpResponse::Ok().json(ConversationDetail {
                conversation,
                history_tokens_estimate,
            })
        }
        Ok(_) => HttpResponse::NotFound().finish(),
        Err(error) => conversation_error(error),
    }
}

#[put("/{conversation_id}")]
async fn rename_conversation(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    store: Data<Arc<SqliteConversationStore>>,
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
    store: Data<Arc<SqliteConversationStore>>,
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
    store: Data<Arc<SqliteConversationStore>>,
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
            match families
                .context_for_member(identity.user_id, family_id)
                .await
            {
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

#[get("/{conversation_id}/runs")]
async fn list_conversation_runs(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    store: Data<Arc<SqliteConversationStore>>,
    manager: Data<RunManager>,
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
    match store.get(id) {
        Ok(conversation) if conversation.owner == owner => {}
        Ok(_) => return HttpResponse::NotFound().finish(),
        Err(error) => return conversation_error(error),
    }

    let runs = manager.list().await.into_iter()
        .filter(|run| run.conversation_id == Some(uuid::Uuid::parse_str(&id.to_string()).expect("ConversationId is a UUID")))
        .map(|run| GetRunResponse {
            run_id: run.id.to_string(),
            agent: run.agent,
            status: match run.status {
                RunStatus::Queued => "queued",
                RunStatus::Running => "running",
                RunStatus::Completed => "completed",
                RunStatus::Failed => "failed",
                RunStatus::Cancelled => "cancelled",
                RunStatus::TimedOut => "timed_out",
                RunStatus::MaxIterationsReached => "max_iterations_reached",
            },
            response: run.response,
            error: run.error,
        })
        .collect::<Vec<_>>();
    HttpResponse::Ok().json(runs)
}

#[post("/{conversation_id}/runs")]
async fn continue_conversation(
    request_http: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    store: Data<Arc<SqliteConversationStore>>,
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
    let user = match identities.get_user(identity.user_id) {
        Ok(Some(user)) => user,
        Ok(None) => return HttpResponse::Unauthorized().finish(),
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };
    let family = match conversation.selected_family_id {
        Some(family_id) => match families
            .context_for_member(identity.user_id, family_id)
            .await
        {
            Ok(mut context) => {
                for member in &mut context.members {
                    let member_id = match UserId::from_str(&member.user_id) {
                        Ok(id) => id,
                        Err(_) => return HttpResponse::InternalServerError().finish(),
                    };
                    member.telegram_id =
                        match identities.find_identity_for_user("telegram", member_id) {
                            Ok(Some(identity)) => Some(identity.subject),
                            Ok(None) => None,
                            Err(_) => return HttpResponse::InternalServerError().finish(),
                        };
                    let user = match identities.get_user(member_id) {
                        Ok(user) => user,
                        Err(_) => return HttpResponse::InternalServerError().finish(),
                    };
                    if let Some(user) = user {
                        member.name = user.display_name;
                        member.birthday = user.birthday;
                    }
                }
                Some(context)
            }
            Err(FamilyServiceError::NotMember | FamilyServiceError::NotFound) => {
                return HttpResponse::Conflict().body("selected family is no longer available");
            }
            Err(_) => return HttpResponse::InternalServerError().finish(),
        },
        None => None,
    };

    let history =
        match conversation_history_for_submission(conversation.messages, input, request.retry) {
            Some(history) => history,
            None => {
                return HttpResponse::BadRequest()
                    .body("the message to retry is not present in this conversation");
            }
        };

    match manager
        .submit(
            conversation.agent.clone(),
            input.to_owned(),
            history,
            ToolContext {
                user_id: Some(identity.user_id.to_string()),
                timezone: user.timezone,
                telegram_id: Some(identity.subject.clone()),
                family,
                conversation_id: Some(id.to_string()),
                run_id: None,
            },
        )
        .await
    {
        Ok(run_id) => {
            auth.register_run(run_id.to_string(), &owner);
            HttpResponse::Accepted().json(SubmitRunResponse {
                run_id: run_id.to_string(),
                status: "queued",
            })
        }
        Err(SubmitRunError::QueueFull) => {
            HttpResponse::ServiceUnavailable().body("run queue is full")
        }
        Err(SubmitRunError::Storage(error)) => {
            tracing::error!(%error, "failed to persist submitted run");
            HttpResponse::InternalServerError().body("could not persist run")
        }
    }
}

pub fn conversations_api() -> Scope {
    web::scope("/conversations")
        .service(list_conversations)
        .service(list_archived_conversations)
        .service(create_conversation)
        .service(archive_conversation)
        .service(unarchive_conversation)
        .service(get_conversation)
        .service(rename_conversation)
        .service(delete_conversation)
        .service(list_conversation_runs)
        .service(select_conversation_family)
        .service(continue_conversation)
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
        SqliteConversationStore, FamilyRelationship, FileUserIdentityStore,
        InMemoryFamilyInvitationStore, InMemoryFamilyStore, UserIdentityStore,
    };
    use serde_json::{Value, json};
    use uuid::Uuid;

    use super::conversations_api;

    #[actix_web::test]
    async fn conversation_family_selection_requires_membership_and_can_be_cleared() {
        let directory = env::temp_dir().join(format!("eme-conversation-family-{}", Uuid::new_v4()));
        let store = Arc::new(SqliteConversationStore::open(directory.join("conversations.sqlite")).unwrap());
        let identities =
            Arc::new(FileUserIdentityStore::open(directory.join("users.json")).unwrap());
        let (user, _) = identities
            .resolve_or_create("telegram", "42", None)
            .unwrap();
        let conversation = store.create("telegram:42", "assistant").unwrap();

        let families = Arc::new(InMemoryFamilyStore::default());
        let invitations = Arc::new(InMemoryFamilyInvitationStore::new(families.clone()));
        let service = Arc::new(FamilyService::new(families, invitations));
        let family = service
            .create_family(user.id, "Home", FamilyRelationship::Father)
            .await
            .unwrap();
        let outsider = identities
            .resolve_or_create("telegram", "84", None)
            .unwrap()
            .0;
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
            test::TestRequest::post()
                .uri("/auth/telegram/start")
                .to_request(),
        )
        .await;
        let login: Value = test::read_body_json(response).await;
        let token = login["token"].as_str().unwrap();
        assert!(auth.complete_login(token, 42, None, identities.as_ref()));
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

#[cfg(test)]
mod retry_history_tests {
    use super::conversation_history_for_submission;
    use agent_runtime::Message;

    #[test]
    fn retry_removes_the_failed_turn_before_resubmitting_it() {
        let history = vec![
            Message::User {
                content: "earlier request".to_owned(),
            },
            Message::User {
                content: "request to retry".to_owned(),
            },
            Message::Assistant {
                content: Some("partial response".to_owned()),
                tool_calls: Vec::new(),
                model_id: None,
                generated_at: None,
            },
        ];

        let retried = conversation_history_for_submission(history, "request to retry", true)
            .expect("matching user message should be found");

        assert_eq!(retried.len(), 1);
        assert!(matches!(
            &retried[0],
            Message::User { content } if content == "earlier request"
        ));
    }

    #[test]
    fn retry_requires_the_original_user_message() {
        assert!(
            conversation_history_for_submission(
                vec![Message::User {
                    content: "another request".to_owned(),
                }],
                "missing request",
                true,
            )
            .is_none()
        );
    }

    #[test]
    fn normal_submission_keeps_the_existing_history() {
        let history = vec![Message::User {
            content: "previous request".to_owned(),
        }];

        let submitted = conversation_history_for_submission(history, "new request", false)
            .expect("normal submissions do not require a matching message");

        assert_eq!(submitted.len(), 1);
    }
}
