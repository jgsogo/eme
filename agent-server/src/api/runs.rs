//! Run submission, inspection, cancellation, and event routes.

use std::str::FromStr;

use super::types::{
    CancelRunResponse, GetRunResponse, RunEventResponse, SubmitRunRequest, SubmitRunResponse,
};
use actix_web::{
    HttpRequest, HttpResponse, Responder, Scope, get, post,
    web::{self, Data, Json, Path},
};
use agent_runtime::ToolContext;
use agent_service::{
    CancelRunError, CancelRunOutcome, RunId, RunManager, RunStatus, SubmitRunError,
};
use agent_storage::{FileUserIdentityStore, UserIdentityStore};
use std::sync::Arc;

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
async fn list_runs(
    request: HttpRequest,
    auth: Option<Data<crate::auth::TelegramAuth>>,
    manager: Data<RunManager>,
) -> impl Responder {
    let owner = auth
        .as_ref()
        .and_then(|auth| auth.owner_for_request(&request));
    let allowed = auth.as_ref().map(|auth| {
        owner
            .as_ref()
            .map(|owner| auth.runs_for_owner(owner))
            .unwrap_or_default()
    });
    let runs = manager
        .list()
        .await
        .into_iter()
        .filter(|run| {
            allowed
                .as_ref()
                .is_none_or(|ids| ids.contains(&run.id.to_string()))
        })
        .collect::<Vec<_>>();
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
async fn submit_run(
    http_request: HttpRequest,
    auth: Option<Data<crate::auth::TelegramAuth>>,
    manager: Data<RunManager>,
    identities: Option<Data<Arc<FileUserIdentityStore>>>,
    request: Json<SubmitRunRequest>,
) -> impl Responder {
    if request.agent.trim().is_empty() {
        return HttpResponse::BadRequest().body("agent must not be empty");
    }
    if request.input.trim().is_empty() {
        return HttpResponse::BadRequest().body("input must not be empty");
    }

    let context = auth
        .as_ref()
        .and_then(|auth| auth.owner_for_request(&http_request))
        .and_then(|owner| owner.strip_prefix("telegram:").map(str::to_owned))
        .and_then(|subject| {
            identities
                .as_ref()?
                .find_identity("telegram", &subject)
                .ok()
                .flatten()
        })
        .map(|identity| ToolContext {
            user_id: Some(identity.user_id.to_string()),
            telegram_id: Some(identity.subject),
            ..ToolContext::default()
        })
        .unwrap_or_default();

    match manager
        .submit(
            request.agent.clone(),
            request.input.clone(),
            Vec::new(),
            context,
        )
        .await
    {
        Ok(run_id) => {
            if let Some(auth) = auth.as_ref() {
                if let Some(owner) = auth.owner_for_request(&http_request) {
                    auth.register_run(run_id.to_string(), &owner);
                }
            }
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
