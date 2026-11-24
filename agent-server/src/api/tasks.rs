//! Authenticated CRUD endpoints and execution history for user-owned scheduled tasks.

use super::types::RunEventResponse;
use std::sync::Arc;

use actix_web::{
    HttpRequest, HttpResponse, Responder, Scope, delete, get, post, put,
    web::{self, Data, Json, Path},
};
use agent_service::{RunManager, RunStatus};
use agent_storage::{FileUserIdentityStore, UserIdentityStore};
use chrono::{DateTime, FixedOffset};
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};
use tasks_tool::{NewTask, TaskError, TaskRegistry};
use uuid::Uuid;

#[derive(Deserialize)]
struct TaskRequest {
    name: String,
    #[serde(default)]
    description: String,
    agent: String,
    prompt: String,
    #[serde(default)]
    schedule: Option<String>,
    #[serde(default)]
    at: Option<DateTime<FixedOffset>>,
    #[serde(default)]
    timezone: Option<String>,
    #[serde(default = "default_enabled")]
    enabled: bool,
}

fn default_enabled() -> bool {
    true
}

impl From<TaskRequest> for NewTask {
    fn from(value: TaskRequest) -> Self {
        Self {
            name: value.name,
            description: value.description,
            agent: value.agent,
            prompt: value.prompt,
            schedule: value.schedule,
            at: value.at,
            timezone: value.timezone,
        }
    }
}

fn actor(
    request: &HttpRequest,
    auth: &crate::auth::TelegramAuth,
    identities: &FileUserIdentityStore,
    tasks: &TaskRegistry,
) -> Result<String, HttpResponse> {
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
    let user_id = identity.user_id.to_string();
    tasks
        .migrate_owner(&owner, &user_id)
        .map_err(error_response)?;
    Ok(user_id)
}

fn error_response(error: TaskError) -> HttpResponse {
    match error {
        TaskError::NotFound => HttpResponse::NotFound().finish(),
        TaskError::Invalid(message) => HttpResponse::BadRequest().body(message),
        TaskError::Storage(_) => HttpResponse::InternalServerError().finish(),
    }
}

#[get("")]
async fn list_tasks(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    tasks: Data<TaskRegistry>,
) -> impl Responder {
    let user = match actor(&request, &auth, &identities, &tasks) {
        Ok(user) => user,
        Err(response) => return response,
    };
    HttpResponse::Ok().json(tasks.list(&user))
}

#[post("")]
async fn create_task(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    tasks: Data<TaskRegistry>,
    body: Json<TaskRequest>,
) -> impl Responder {
    let user = match actor(&request, &auth, &identities, &tasks) {
        Ok(user) => user,
        Err(response) => return response,
    };
    let body = body.into_inner();
    let enabled = body.enabled;
    match tasks.add(&user, body.into()) {
        Ok(task) if enabled => HttpResponse::Created().json(task),
        Ok(task) => match tasks.set_enabled(&user, task.id, false) {
            Ok(task) => HttpResponse::Created().json(task),
            Err(error) => error_response(error),
        },
        Err(error) => error_response(error),
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

fn unix_ms(value: SystemTime) -> u128 {
    value
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[get("/{task_id}/executions")]
async fn list_task_executions(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    tasks: Data<TaskRegistry>,
    manager: Data<RunManager>,
    task_id: Path<String>,
) -> impl Responder {
    let user = match actor(&request, &auth, &identities, &tasks) {
        Ok(user) => user,
        Err(response) => return response,
    };
    let task_id = match Uuid::parse_str(&task_id) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().body("invalid task identifier"),
    };
    if !tasks.list(&user).iter().any(|task| task.id == task_id) {
        return HttpResponse::NotFound().finish();
    }

    let runs = match manager.task_runs(task_id).await {
        Ok(runs) => runs,
        Err(error) => {
            tracing::error!(%task_id, %error, "failed to load task execution history");
            return HttpResponse::InternalServerError()
                .body("could not load task execution history");
        }
    };
    let mut executions = Vec::with_capacity(runs.len());
    for run in runs {
        let events = match manager.events(run.id).await {
            Some(events) => events
                .into_iter()
                .map(RunEventResponse::from)
                .collect::<Vec<_>>(),
            None => {
                tracing::error!(run_id = %run.id, "failed to load task execution events");
                return HttpResponse::InternalServerError()
                    .body("could not load execution events");
            }
        };
        let usage = match manager.usage_records(Some(run.id)).await {
            Ok(records) => records,
            Err(error) => {
                tracing::error!(run_id = %run.id, %error, "failed to load task execution usage");
                return HttpResponse::InternalServerError()
                    .body("could not load execution usage");
            }
        };
        let duration_ms = run
            .started_at
            .zip(run.finished_at)
            .map(|(start, finish)| {
                finish.duration_since(start).unwrap_or_default().as_millis()
            });
        executions.push(serde_json::json!({
            "run_id": run.id.to_string(),
            "agent": run.agent,
            "input": run.input,
            "status": status_name(run.status),
            "response": run.response,
            "error": run.error,
            "created_at_unix_ms": unix_ms(run.created_at),
            "started_at_unix_ms": run.started_at.map(unix_ms),
            "finished_at_unix_ms": run.finished_at.map(unix_ms),
            "duration_ms": duration_ms,
            "events": events,
            "usage": usage,
        }));
    }
    HttpResponse::Ok().json(executions)
}

#[put("/{task_id}")]
async fn update_task(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    tasks: Data<TaskRegistry>,
    task_id: Path<String>,
    body: Json<TaskRequest>,
) -> impl Responder {
    let user = match actor(&request, &auth, &identities, &tasks) {
        Ok(user) => user,
        Err(response) => return response,
    };
    let id = match Uuid::parse_str(&task_id) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().body("invalid task identifier"),
    };
    let body = body.into_inner();
    let enabled = body.enabled;
    match tasks.update(&user, id, body.into(), enabled) {
        Ok(task) => HttpResponse::Ok().json(task),
        Err(error) => error_response(error),
    }
}

#[delete("/{task_id}")]
async fn delete_task(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    tasks: Data<TaskRegistry>,
    task_id: Path<String>,
) -> impl Responder {
    let user = match actor(&request, &auth, &identities, &tasks) {
        Ok(user) => user,
        Err(response) => return response,
    };
    let id = match Uuid::parse_str(&task_id) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().body("invalid task identifier"),
    };
    match tasks.remove(&user, id) {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(error) => error_response(error),
    }
}

pub fn tasks_api() -> Scope {
    web::scope("/tasks")
        .service(list_tasks)
        .service(create_task)
        .service(list_task_executions)
        .service(update_task)
        .service(delete_task)
}
