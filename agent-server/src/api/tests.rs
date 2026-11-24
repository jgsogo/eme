//! Integration tests for the HTTP API route scopes.

use std::{sync::Arc, time::Duration};

use actix_web::{App, http::StatusCode, test, web};
use agent_models::ModelCapability;
use agent_runtime::{CancellationToken, Message, ToolContext};
use agent_service::{ExecutionOutcome, ExecutionReport, RunExecutor, RunId, RunManager};
use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::sync::Notify;

use super::{
    AgentCatalog, AgentListItem, agents_api, meal_planner_api, runs_api, school_menus_api,
};
use meal_planner::MealPlannerStore;

struct ImmediateExecutor;

#[async_trait]
impl RunExecutor for ImmediateExecutor {
    async fn execute(
        &self,
        _run_id: RunId,
        _agent: String,
        input: String,
        _history: Vec<Message>,
        _context: ToolContext,
        _cancellation: CancellationToken,
    ) -> Result<ExecutionReport, String> {
        Ok(ExecutionReport {
            outcome: ExecutionOutcome::Completed(Some(format!("echo: {input}"))),
            runtime_events: Vec::new(),
            messages: None,
        })
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
        _history: Vec<Message>,
        _context: ToolContext,
        cancellation: CancellationToken,
    ) -> Result<ExecutionReport, String> {
        self.started.notify_one();
        loop {
            if cancellation.is_cancelled() {
                return Ok(ExecutionReport {
                    outcome: ExecutionOutcome::Cancelled,
                    runtime_events: Vec::new(),
                    messages: None,
                });
            }
            tokio::select! {
                _ = self.release.notified() => {
                    return Ok(ExecutionReport {
                        outcome: ExecutionOutcome::Completed(None),
                        runtime_events: Vec::new(),
                        messages: None,
                    });
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
async fn school_menu_scope_is_not_shadowed_by_meal_planner_scope() {
    let store = MealPlannerStore::open_in_memory().unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(store))
            // Keep this order aligned with the server's route registration.
            .service(school_menus_api())
            .service(meal_planner_api()),
    )
    .await;

    let request = test::TestRequest::get()
        .uri("/meal-planner/school-menus/?from=2026-11-16&to=2026-11-22")
        .to_request();
    let response = test::call_service(&app, request).await;
    // The route must resolve to the school-menu handler rather than the
    // overlapping meal-planner scope. This isolated route test intentionally
    // omits the authentication and family-service state required by the
    // handler, so a server error is expected after route matching.
    assert_ne!(response.status(), StatusCode::NOT_FOUND);
    assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
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
        capabilities: vec![ModelCapability::Chat, ModelCapability::Reasoning],
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
    assert_eq!(body[0]["capabilities"], json!(["chat", "reasoning"]));
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
async fn streams_run_events_and_resumes_after_last_event_id() {
    let manager = manager();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(manager.clone()))
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
    let parsed_run_id = run_id.parse::<RunId>().unwrap();

    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if manager
                .get(parsed_run_id)
                .await
                .is_some_and(|run| run.status == agent_service::RunStatus::Completed)
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("run should complete before replay");

    let request = test::TestRequest::get()
        .uri(&format!("/runs/{run_id}/events/stream"))
        .to_request();
    let response = test::call_service(&app, request).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "text/event-stream"
    );
    let body = test::read_body(response).await;
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(body.contains("id: 0\n"));
    assert!(body.contains("id: 1\n"));
    assert!(body.contains("id: 2\n"));
    assert!(body.contains("queued"));
    assert!(body.contains("completed"));

    let request = test::TestRequest::get()
        .uri(&format!("/runs/{run_id}/events/stream"))
        .insert_header(("Last-Event-ID", "0"))
        .to_request();
    let response = test::call_service(&app, request).await;
    let body = test::read_body(response).await;
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(!body.contains("id: 0\n"));
    assert!(body.contains("id: 1\n"));
    assert!(body.contains("id: 2\n"));
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
