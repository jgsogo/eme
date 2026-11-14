//! HTTP API for submitting, inspecting, and cancelling agent runs.

use std::str::FromStr;

use actix_web::{
    HttpResponse, Responder, Scope, get, post,
    web::{self, Data, Json, Path},
};
use agent_service::{
    CancelRunError, CancelRunOutcome, RunId, RunManager, RunStatus, SubmitRunError,
};
use serde::{Deserialize, Serialize};

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
pub struct CancelRunResponse {
    pub run_id: String,
    pub status: &'static str,
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

/// Builds the /runs route scope. Register RunManager as application data
/// before mounting this scope.
pub fn runs_api() -> Scope {
    web::scope("/runs")
        .service(submit_run)
        .service(get_run)
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

    use super::runs_api;

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
