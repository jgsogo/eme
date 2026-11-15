//! HTTP API for submitting, inspecting, and cancelling agent runs.

pub mod auth;

use std::{str::FromStr, sync::Arc, time::Duration};

use actix_web::{
    HttpRequest, HttpResponse, Responder, Scope, get, post,
    web::{self, Data, Json, Path},
};
use agent_service::{
    CancelRunError, CancelRunOutcome, RunEvent, RunEventKind, RunId, RunManager, RunStatus,
    SubmitRunError,
};
use agent_storage::{Conversation, ConversationId, ConversationStore, ConversationStoreError};
use serde::{Deserialize, Serialize};
use tokio::time::sleep;

#[derive(Debug, Deserialize)]
pub struct SubmitRunRequest {
    pub agent: String,
    pub input: String,
}

#[derive(Debug, Serialize)]
pub struct SubmitRunResponse {
    pub run_id: String,
    pub status: &'static str,
}

#[derive(Debug, Serialize)]
pub struct GetRunResponse {
    pub run_id: String,
    pub agent: String,
    pub status: &'static str,
    pub response: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RunEventResponse {
    pub sequence: u64,
    pub kind: &'static str,
    pub iteration: Option<usize>,
    pub tool_name: Option<String>,
    pub error_category: Option<&'static str>,
    pub occurred_at_unix_ms: u128,
}

impl From<RunEvent> for RunEventResponse {
    fn from(event: RunEvent) -> Self {
        let occurred_at_unix_ms = event
            .occurred_at
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();

        Self {
            sequence: event.sequence,
            kind: event_kind_name(event.kind),
            iteration: event.iteration,
            tool_name: event.tool_name,
            error_category: event.error_category,
            occurred_at_unix_ms,
        }
    }
}

fn event_kind_name(kind: RunEventKind) -> &'static str {
    match kind {
        RunEventKind::Queued => "queued",
        RunEventKind::Started => "started",
        RunEventKind::Completed => "completed",
        RunEventKind::Failed => "failed",
        RunEventKind::Cancelled => "cancelled",
        RunEventKind::TimedOut => "timed_out",
        RunEventKind::MaxIterationsReached => "max_iterations_reached",
        RunEventKind::ModelRequested => "model_requested",
        RunEventKind::ModelResponded => "model_responded",
        RunEventKind::ToolExecutionStarted => "tool_execution_started",
        RunEventKind::ToolExecutionFinished => "tool_execution_finished",
        RunEventKind::ModelFailed => "model_failed",
        RunEventKind::RuntimeCancelled => "runtime_cancelled",
        RunEventKind::RuntimeTimedOut => "runtime_timed_out",
    }
}

#[derive(Debug, Serialize)]
pub struct CancelRunResponse {
    pub run_id: String,
    pub status: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentListItem {
    pub name: String,
    pub description: String,
    pub model: String,
    pub tools: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AgentCatalog {
    agents: std::sync::Arc<[AgentListItem]>,
}

impl AgentCatalog {
    pub fn new(agents: impl IntoIterator<Item = AgentListItem>) -> Self {
        let mut agents: Vec<_> = agents.into_iter().collect();
        agents.sort_by(|left, right| left.name.cmp(&right.name));
        Self {
            agents: agents.into(),
        }
    }

    pub fn list(&self) -> &[AgentListItem] {
        &self.agents
    }
}

fn status_name(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Queued => "queued",
        RunStatus::Running => "running",
        RunStatus::Completed => "completed",
        RunStatus::Failed => "failed",
        RunStatus::Cancelled => "cancelled",
        RunStatus::TimedOut => "timed_out",
        RunStatus::MaxIterationsReached => "max_iterations_reached",
    }
}

#[get("")]
async fn list_runs(request: HttpRequest, auth: Option<Data<crate::auth::TelegramAuth>>, manager: Data<RunManager>) -> impl Responder {
    let owner = auth.as_ref().and_then(|auth| auth.owner_for_request(&request));
    let allowed = auth.as_ref().map(|auth| owner.as_ref().map(|owner| auth.runs_for_owner(owner)).unwrap_or_default());
    let runs = manager.list().await.into_iter().filter(|run| allowed.as_ref().is_none_or(|ids| ids.contains(&run.id.to_string()))).collect::<Vec<_>>();
    let body = runs
        .into_iter()
        .map(|run| GetRunResponse {
            run_id: run.id.to_string(),
            agent: run.agent,
            status: status_name(run.status),
            response: run.response,
            error: run.error,
        })
        .collect::<Vec<_>>();
    HttpResponse::Ok().json(body)
}

#[post("")]
async fn submit_run(manager: Data<RunManager>, request: Json<SubmitRunRequest>) -> impl Responder {
    if request.agent.trim().is_empty() {
        return HttpResponse::BadRequest().body("agent must not be empty");
    }
    if request.input.trim().is_empty() {
        return HttpResponse::BadRequest().body("input must not be empty");
    }

    match manager
        .submit(request.agent.clone(), request.input.clone())
        .await
    {
        Ok(run_id) => HttpResponse::Accepted().json(SubmitRunResponse {
            run_id: run_id.to_string(),
            status: "queued",
        }),
        Err(SubmitRunError::QueueFull) => {
            HttpResponse::ServiceUnavailable().body("run queue is full")
        }
    }
}

#[get("/{run_id}")]
async fn get_run(manager: Data<RunManager>, run_id: Path<String>) -> impl Responder {
    let run_id = match RunId::from_str(&run_id) {
        Ok(run_id) => run_id,
        Err(_) => return HttpResponse::BadRequest().body("invalid run identifier"),
    };

    match manager.get(run_id).await {
        Some(run) => HttpResponse::Ok().json(GetRunResponse {
            run_id: run.id.to_string(),
            agent: run.agent,
            status: status_name(run.status),
            response: run.response,
            error: run.error,
        }),
        None => HttpResponse::NotFound().finish(),
    }
}

#[get("/{run_id}/events")]
async fn get_run_events(manager: Data<RunManager>, run_id: Path<String>) -> impl Responder {
    let run_id = match RunId::from_str(&run_id) {
        Ok(run_id) => run_id,
        Err(_) => return HttpResponse::BadRequest().body("invalid run identifier"),
    };

    match manager.events(run_id).await {
        Some(events) => HttpResponse::Ok().json(
            events
                .into_iter()
                .map(RunEventResponse::from)
                .collect::<Vec<_>>(),
        ),
        None => HttpResponse::NotFound().finish(),
    }
}

#[post("/{run_id}/cancel")]
async fn cancel_run(manager: Data<RunManager>, run_id: Path<String>) -> impl Responder {
    let run_id = match RunId::from_str(&run_id) {
        Ok(run_id) => run_id,
        Err(_) => return HttpResponse::BadRequest().body("invalid run identifier"),
    };

    match manager.cancel(run_id).await {
        Ok(outcome) => {
            let run = manager
                .get(run_id)
                .await
                .expect("cancelled run remains available");
            let response = CancelRunResponse {
                run_id: run.id.to_string(),
                status: status_name(run.status),
            };

            match outcome {
                CancelRunOutcome::Requested | CancelRunOutcome::CancelledBeforeStart => {
                    HttpResponse::Accepted().json(response)
                }
                CancelRunOutcome::AlreadyRequested | CancelRunOutcome::AlreadyFinished => {
                    HttpResponse::Ok().json(response)
                }
            }
        }
        Err(CancelRunError::NotFound) => HttpResponse::NotFound().finish(),
    }
}

fn authenticated_owner(request: &HttpRequest, auth: &crate::auth::TelegramAuth) -> Result<String, HttpResponse> {
    auth.owner_for_request(request).ok_or_else(|| HttpResponse::Unauthorized().body("authentication required"))
}

#[derive(Debug, Deserialize)]
pub struct CreateConversationRequest {
    pub agent: String,
}

#[derive(Debug, Deserialize)]
pub struct ContinueConversationRequest {
    pub input: String,
}

#[derive(Debug, Serialize)]
pub struct ConversationSummary {
    pub id: String,
    pub agent: String,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: usize,
}

impl From<Conversation> for ConversationSummary {
    fn from(conversation: Conversation) -> Self {
        Self {
            id: conversation.id.to_string(),
            agent: conversation.agent,
            created_at: conversation.created_at.to_rfc3339(),
            updated_at: conversation.updated_at.to_rfc3339(),
            message_count: conversation.messages.len(),
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
async fn list_conversations(request: HttpRequest, auth: Data<crate::auth::TelegramAuth>, store: Data<Arc<ConversationStore>>) -> impl Responder {
    let owner = match authenticated_owner(&request, &auth) { Ok(owner) => owner, Err(response) => return response };
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
    let owner = match authenticated_owner(&request_http, &auth) { Ok(owner) => owner, Err(response) => return response };
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
    let owner = match authenticated_owner(&request, &auth) { Ok(owner) => owner, Err(response) => return response };
    let id = match ConversationId::from_str(conversation_id.as_str()) {
        Ok(id) => id,
        Err(_) => {
            return HttpResponse::BadRequest().body("invalid conversation identifier");
        }
    };

    match store.get(id) {
        Ok(conversation) if conversation.owner == owner => {
            HttpResponse::Ok().json(conversation)
        }
        Ok(_) => HttpResponse::NotFound().finish(),
        Err(error) => conversation_error(error),
    }
}

#[post("/{conversation_id}/runs")]
async fn continue_conversation(
    request_http: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    store: Data<Arc<ConversationStore>>,
    manager: Data<RunManager>,
    conversation_id: Path<String>,
    request: Json<ContinueConversationRequest>,
) -> impl Responder {
    let owner = match authenticated_owner(&request_http, &auth) { Ok(owner) => owner, Err(response) => return response };
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

    match manager
        .submit_with_history(
            conversation.agent.clone(),
            input.to_owned(),
            conversation.messages,
        )
        .await
    {
        Ok(run_id) => {
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
        .service(continue_conversation)
}

#[get("")]
async fn list_agents(catalog: Data<AgentCatalog>) -> impl Responder {
    HttpResponse::Ok().json(catalog.list())
}

/// Builds the /agents route scope. Register AgentCatalog as application data
/// before mounting this scope.
pub fn agents_api() -> Scope {
    web::scope("/agents").service(list_agents)
}

/// Builds the /runs route scope. Register RunManager as application data
/// before mounting this scope.
pub fn runs_api() -> Scope {
    web::scope("/runs")
        .service(list_runs)
        .service(submit_run)
        .service(get_run)
        .service(get_run_events)
        .service(cancel_run)
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use actix_web::{App, http::StatusCode, test, web};
    use agent_runtime::CancellationToken;
    use agent_service::{ExecutionOutcome, RunExecutor, RunId, RunManager};
    use async_trait::async_trait;
    use serde_json::{Value, json};
    use tokio::sync::Notify;

    use super::{AgentCatalog, AgentListItem, agents_api, runs_api};

    struct ImmediateExecutor;

    #[async_trait]
    impl RunExecutor for ImmediateExecutor {
        async fn execute(
            &self,
            _run_id: RunId,
            _agent: String,
            input: String,
            _cancellation: CancellationToken,
        ) -> Result<ExecutionOutcome, String> {
            Ok(ExecutionOutcome::Completed(Some(format!("echo: {input}"))))
        }
    }

    struct WaitingExecutor {
        started: Arc<Notify>,
        release: Arc<Notify>,
    }

    #[async_trait]
    impl RunExecutor for WaitingExecutor {
        async fn execute(
            &self,
            _run_id: RunId,
            _agent: String,
            _input: String,
            cancellation: CancellationToken,
        ) -> Result<ExecutionOutcome, String> {
            self.started.notify_one();
            loop {
                if cancellation.is_cancelled() {
                    return Ok(ExecutionOutcome::Cancelled);
                }
                tokio::select! {
                    _ = self.release.notified() => {
                        return Ok(ExecutionOutcome::Completed(None));
                    }
                    _ = tokio::time::sleep(Duration::from_millis(1)) => {}
                }
            }
        }
    }

    fn manager() -> RunManager {
        RunManager::new(Arc::new(ImmediateExecutor), 1, 8).expect("valid run manager configuration")
    }

    #[actix_web::test]
    async fn submits_run_and_returns_stable_identifier() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(manager()))
                .service(runs_api()),
        )
        .await;

        let request = test::TestRequest::post()
            .uri("/runs")
            .set_json(json!({"agent": "assistant", "input": "hello"}))
            .to_request();
        let response = test::call_service(&app, request).await;

        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body: Value = test::read_body_json(response).await;
        let run_id = body["run_id"].as_str().expect("run ID should be a string");
        assert_eq!(body["status"], "queued");

        let request = test::TestRequest::get()
            .uri(&format!("/runs/{run_id}"))
            .to_request();
        let response = test::call_service(&app, request).await;
        assert_eq!(response.status(), StatusCode::OK);

        let body: Value = test::read_body_json(response).await;
        assert_eq!(body["run_id"], run_id);
        assert_eq!(body["agent"], "assistant");
        assert!(["queued", "running", "completed"].contains(&body["status"].as_str().unwrap()));

        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let request = test::TestRequest::get()
                    .uri(&format!("/runs/{run_id}"))
                    .to_request();
                let response = test::call_service(&app, request).await;
                let body: Value = test::read_body_json(response).await;
                if body["status"] == "completed" {
                    assert_eq!(body["response"], "echo: hello");
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("run should complete");
    }

    #[actix_web::test]
    async fn lists_submitted_runs() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(manager()))
                .service(runs_api()),
        )
        .await;

        let request = test::TestRequest::post()
            .uri("/runs")
            .set_json(json!({"agent": "assistant", "input": "hello"}))
            .to_request();
        let response = test::call_service(&app, request).await;
        let body: Value = test::read_body_json(response).await;
        let run_id = body["run_id"].as_str().unwrap().to_owned();

        let request = test::TestRequest::get().uri("/runs").to_request();
        let response = test::call_service(&app, request).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = test::read_body_json(response).await;
        let runs = body.as_array().expect("list should be an array");
        assert!(
            runs.iter().any(|run| run["run_id"] == run_id),
            "submitted run should appear in the list"
        );
    }

    #[actix_web::test]
    async fn lists_configured_agents() {
        let catalog = AgentCatalog::new([AgentListItem {
            name: "assistant".into(),
            description: "General assistant".into(),
            model: "test/model".into(),
            tools: vec!["tg_notify_user".into()],
        }]);
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(catalog))
                .service(agents_api()),
        )
        .await;

        let request = test::TestRequest::get().uri("/agents").to_request();
        let response = test::call_service(&app, request).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = test::read_body_json(response).await;
        assert_eq!(body[0]["name"], "assistant");
        assert_eq!(body[0]["model"], "test/model");
        assert_eq!(body[0]["tools"][0], "tg_notify_user");
    }

    #[actix_web::test]
    async fn exposes_ordered_run_lifecycle_events() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(manager()))
                .service(runs_api()),
        )
        .await;

        let request = test::TestRequest::post()
            .uri("/runs")
            .set_json(json!({"agent": "assistant", "input": "hello"}))
            .to_request();
        let response = test::call_service(&app, request).await;
        let body: Value = test::read_body_json(response).await;
        let run_id = body["run_id"].as_str().unwrap().to_owned();

        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let request = test::TestRequest::get()
                    .uri(&format!("/runs/{run_id}/events"))
                    .to_request();
                let response = test::call_service(&app, request).await;
                let events: Value = test::read_body_json(response).await;
                if events.as_array().is_some_and(|events| {
                    events
                        .last()
                        .is_some_and(|event| event["kind"] == "completed")
                }) {
                    assert_eq!(events[0]["sequence"], 0);
                    assert_eq!(events[1]["kind"], "started");
                    assert_eq!(events[2]["kind"], "completed");
                    assert!(events[0]["occurred_at_unix_ms"].is_number());
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("lifecycle events should be recorded in order");
    }

    #[actix_web::test]
    async fn rejects_invalid_and_unknown_event_targets() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(manager()))
                .service(runs_api()),
        )
        .await;

        let request = test::TestRequest::get()
            .uri("/runs/not-a-uuid/events")
            .to_request();
        assert_eq!(
            test::call_service(&app, request).await.status(),
            StatusCode::BAD_REQUEST
        );

        let request = test::TestRequest::get()
            .uri("/runs/00000000-0000-0000-0000-000000000000/events")
            .to_request();
        assert_eq!(
            test::call_service(&app, request).await.status(),
            StatusCode::NOT_FOUND
        );
    }

    #[actix_web::test]
    async fn cancels_a_running_run_and_returns_accepted() {
        let executor = WaitingExecutor {
            started: Arc::new(Notify::new()),
            release: Arc::new(Notify::new()),
        };
        let started = Arc::clone(&executor.started);
        let manager = RunManager::new(Arc::new(executor), 1, 4).unwrap();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(manager))
                .service(runs_api()),
        )
        .await;

        let request = test::TestRequest::post()
            .uri("/runs")
            .set_json(json!({"agent": "assistant", "input": "hello"}))
            .to_request();
        let response = test::call_service(&app, request).await;
        let body: Value = test::read_body_json(response).await;
        let run_id = body["run_id"].as_str().unwrap().to_owned();
        started.notified().await;

        let request = test::TestRequest::post()
            .uri(&format!("/runs/{run_id}/cancel"))
            .to_request();
        let response = test::call_service(&app, request).await;
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body: Value = test::read_body_json(response).await;
        assert_eq!(body["status"], "running");

        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let request = test::TestRequest::get()
                    .uri(&format!("/runs/{run_id}"))
                    .to_request();
                let response = test::call_service(&app, request).await;
                let body: Value = test::read_body_json(response).await;
                if body["status"] == "cancelled" {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("run should become cancelled");
    }

    #[actix_web::test]
    async fn rejects_invalid_and_unknown_cancellation_targets() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(manager()))
                .service(runs_api()),
        )
        .await;

        let request = test::TestRequest::post()
            .uri("/runs/not-a-uuid/cancel")
            .to_request();
        assert_eq!(
            test::call_service(&app, request).await.status(),
            StatusCode::BAD_REQUEST
        );

        let request = test::TestRequest::post()
            .uri("/runs/00000000-0000-0000-0000-000000000000/cancel")
            .to_request();
        assert_eq!(
            test::call_service(&app, request).await.status(),
            StatusCode::NOT_FOUND
        );
    }

    #[actix_web::test]
    async fn rejects_invalid_requests_and_identifiers() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(manager()))
                .service(runs_api()),
        )
        .await;

        let request = test::TestRequest::post()
            .uri("/runs")
            .set_json(json!({"agent": "", "input": "hello"}))
            .to_request();
        assert_eq!(
            test::call_service(&app, request).await.status(),
            StatusCode::BAD_REQUEST
        );

        let request = test::TestRequest::get()
            .uri("/runs/not-a-uuid")
            .to_request();
        assert_eq!(
            test::call_service(&app, request).await.status(),
            StatusCode::BAD_REQUEST
        );

        let request = test::TestRequest::get()
            .uri("/runs/00000000-0000-0000-0000-000000000000")
            .to_request();
        assert_eq!(
            test::call_service(&app, request).await.status(),
            StatusCode::NOT_FOUND
        );
    }
}
