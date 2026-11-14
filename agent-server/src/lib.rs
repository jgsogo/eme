//! HTTP API for submitting and inspecting agent runs.

use std::str::FromStr;

use actix_web::{
    HttpResponse, Responder, Scope, get, post,
    web::{self, Data, Json, Path},
};
use agent_service::{RunId, RunManager, RunStatus, SubmitRunError};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

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

#[post("")]
async fn submit_run(manager: Data<RunManager>, request: Json<SubmitRunRequest>) -> impl Responder {
    if request.agent.trim().is_empty() {
        warn!("rejected run submission with empty agent");
        return HttpResponse::BadRequest().body("agent must not be empty");
    }
    if request.input.trim().is_empty() {
        warn!("rejected run submission with empty input");
        return HttpResponse::BadRequest().body("input must not be empty");
    }

    match manager
        .submit(request.agent.clone(), request.input.clone())
        .await
    {
        Ok(run_id) => {
            info!(run_id = %run_id, agent = %request.agent, "accepted run submission");
            HttpResponse::Accepted().json(SubmitRunResponse {
                run_id: run_id.to_string(),
                status: "queued",
            })
        }
        Err(SubmitRunError::QueueFull) => {
            warn!(agent = %request.agent, "run submission rejected because queue is full");
            HttpResponse::ServiceUnavailable().body("run queue is full")
        }
    }
}

#[get("/{run_id}")]
async fn get_run(manager: Data<RunManager>, run_id: Path<String>) -> impl Responder {
    let run_id = match RunId::from_str(&run_id) {
        Ok(run_id) => run_id,
        Err(_) => {
            warn!("rejected request with invalid run identifier");
            return HttpResponse::BadRequest().body("invalid run identifier");
        }
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

/// Builds the /runs route scope. Register RunManager as application data
/// before mounting this scope.
pub fn runs_api() -> Scope {
    web::scope("/runs").service(submit_run).service(get_run)
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use actix_web::{App, http::StatusCode, test, web};
    use agent_service::{ExecutionOutcome, RunExecutor, RunId, RunManager};
    use async_trait::async_trait;
    use serde_json::{Value, json};

    use super::runs_api;

    struct ImmediateExecutor;

    #[async_trait]
    impl RunExecutor for ImmediateExecutor {
        async fn execute(
            &self,
            _run_id: RunId,
            _agent: String,
            input: String,
        ) -> Result<ExecutionOutcome, String> {
            Ok(ExecutionOutcome::Completed(Some(format!("echo: {input}"))))
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
