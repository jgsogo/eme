//! Authenticated CRUD endpoints for user-owned scheduled tasks.

use std::sync::Arc;

use actix_web::{
    HttpRequest, HttpResponse, Responder, Scope, delete, get, post, put,
    web::{self, Data, Json, Path},
};
use agent_storage::{FileUserIdentityStore, UserIdentityStore};
use chrono::{DateTime, FixedOffset};
use serde::Deserialize;
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

fn default_enabled() -> bool { true }

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
) -> Result<String, HttpResponse> {
    let owner = auth.owner_for_request(request)
        .ok_or_else(|| HttpResponse::Unauthorized().finish())?;
    let subject = owner.strip_prefix("telegram:")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| HttpResponse::Unauthorized().finish())?;
    let identity = identities.find_identity("telegram", subject)
        .map_err(|_| HttpResponse::InternalServerError().finish())?
        .ok_or_else(|| HttpResponse::Unauthorized().finish())?;
    Ok(identity.user_id.to_string())
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
    let user = match actor(&request, &auth, &identities) {
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
    let user = match actor(&request, &auth, &identities) {
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

#[put("/{task_id}")]
async fn update_task(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    tasks: Data<TaskRegistry>,
    task_id: Path<String>,
    body: Json<TaskRequest>,
) -> impl Responder {
    let user = match actor(&request, &auth, &identities) {
        Ok(user) => user,
        Err(response) => return response,
    };
    let id = match Uuid::parse_str(&task_id) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().body("invalid task identifier"),
    };
    let body = body.into_inner();
    match tasks.update(&user, id, body.into(), body.enabled) {
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
    let user = match actor(&request, &auth, &identities) {
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
        .service(update_task)
        .service(delete_task)
}
