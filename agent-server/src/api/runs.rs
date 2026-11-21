//! Run submission, inspection, cancellation, and event routes.

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use agent_metering::{CostEstimate, CostStatus, ModelPricing, calculate_cost};

use super::types::{
    CancelRunResponse, GetRunResponse, RunEventResponse, SubmitRunRequest, SubmitRunResponse,
};
use actix_web::{
    HttpRequest, HttpResponse, Responder, Scope, get, post,
    web::{self, Data, Json, Path},
};
use agent_runtime::ToolContext;
use agent_service::Run;
use agent_service::{
    CancelRunError, CancelRunOutcome, RunId, RunManager, RunStatus, SubmitRunError,
};
use agent_storage::{
    ConversationId, FileUserIdentityStore, SqliteConversationStore, UserIdentityStore,
};
use std::sync::Arc;

fn owns_run(
    run: &Run,
    request: &HttpRequest,
    auth: Option<&crate::auth::TelegramAuth>,
    store: Option<&SqliteConversationStore>,
) -> bool {
    let Some(auth) = auth else {
        return true;
    };
    let Some(owner) = auth.owner_for_request(request) else {
        return false;
    };
    if let Some(conversation_id) = run.conversation_id {
        let Ok(id) = ConversationId::from_str(&conversation_id.to_string()) else {
            return false;
        };
        store.is_some_and(|store| {
            store
                .get(id)
                .is_ok_and(|conversation| conversation.owner == owner)
        })
    } else {
        auth.runs_for_owner(&owner).contains(&run.id.to_string())
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
async fn list_runs(
    request: HttpRequest,
    auth: Option<Data<crate::auth::TelegramAuth>>,
    manager: Data<RunManager>,
    store: Option<Data<Arc<SqliteConversationStore>>>,
) -> impl Responder {
    let runs = manager
        .list()
        .await
        .into_iter()
        .filter(|run| match auth.as_ref() {
            Some(auth) => owns_run(
                run,
                &request,
                Some(auth.get_ref()),
                store.as_ref().map(|store| store.get_ref().as_ref()),
            ),
            None => true,
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

#[get("/usage")]
async fn get_usage(
    request: HttpRequest,
    auth: Option<Data<crate::auth::TelegramAuth>>,
    store: Option<Data<Arc<SqliteConversationStore>>>,
    manager: Data<RunManager>,
) -> impl Responder {
    let visible_run_ids = manager.list().await.into_iter()
        .filter(|run| owns_run(
            run,
            &request,
            auth.as_deref().map(|value| &**value),
            store.as_ref().map(|value| value.get_ref().as_ref()),
        ))
        .map(|run| run.id.as_uuid())
        .collect::<HashSet<_>>();

    let records = match manager.usage_records(None).await {
        Ok(records) => records,
        Err(error) => {
            tracing::error!(%error, "failed to load usage records");
            return HttpResponse::InternalServerError().body("could not load usage records");
        }
    };

    let mut pricing_cache: HashMap<(String, String), Vec<ModelPricing>> = HashMap::new();
    let mut response = Vec::new();
    for record in records.into_iter().filter(|record| visible_run_ids.contains(&record.run_id)) {
        let cost = if let Some(reported) = &record.reported_cost {
            Some(CostEstimate::from_reported(reported))
        } else {
            let key = (record.provider.clone(), record.model.clone());
            if !pricing_cache.contains_key(&key) {
                match manager.pricing_for(&key.0, &key.1).await {
                    Ok(pricing) => { pricing_cache.insert(key.clone(), pricing); }
                    Err(error) => {
                        tracing::error!(provider = %key.0, model = %key.1, %error, "failed to load model pricing");
                        pricing_cache.insert(key.clone(), Vec::new());
                    }
                }
            }
            pricing_cache.get(&key).and_then(|versions| {
                versions.iter().find(|pricing| pricing.applies_to(&record))
                    .and_then(|pricing| calculate_cost(&record, pricing).ok())
                    .filter(|cost| cost.status != CostStatus::Unavailable)
            })
        };

        let mut item = match serde_json::to_value(&record) {
            Ok(value) => value,
            Err(error) => {
                tracing::error!(%error, "failed to serialize usage record");
                return HttpResponse::InternalServerError().body("could not serialize usage records");
            }
        };
        item["cost"] = serde_json::to_value(cost).unwrap_or(serde_json::Value::Null);
        response.push(item);
    }

    HttpResponse::Ok().json(response)
}

#[get("/pricing")]
async fn list_pricing(manager: Data<RunManager>) -> impl Responder {
    match manager.pricing_catalog().await {
        Ok(pricing) => HttpResponse::Ok().json(pricing),
        Err(error) => {
            tracing::error!(%error, "failed to load pricing catalogue");
            HttpResponse::InternalServerError().body("could not load pricing catalogue")
        }
    }
}

#[post("/pricing")]
async fn create_pricing(
    request: HttpRequest,
    auth: Option<Data<crate::auth::TelegramAuth>>,
    manager: Data<RunManager>,
    pricing: Json<ModelPricing>,
) -> impl Responder {
    if auth.as_ref().and_then(|auth| auth.owner_for_request(&request)).is_none() {
        return HttpResponse::Unauthorized().finish();
    }
    if let Err(error) = pricing.validate() {
        return HttpResponse::BadRequest().body(error.to_string());
    }
    match manager.register_pricing(pricing.into_inner()).await {
        Ok(()) => HttpResponse::Created().finish(),
        Err(error) if error.contains("already exists") => HttpResponse::Conflict().body(error),
        Err(error) => {
            tracing::error!(%error, "failed to register model pricing");
            HttpResponse::InternalServerError().body("could not register model pricing")
        }
    }
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
async fn get_run(
    request: HttpRequest,
    auth: Option<Data<crate::auth::TelegramAuth>>,
    store: Option<Data<Arc<SqliteConversationStore>>>,
    manager: Data<RunManager>,
    run_id: Path<String>,
) -> impl Responder {
    let run_id = match RunId::from_str(&run_id) {
        Ok(run_id) => run_id,
        Err(_) => return HttpResponse::BadRequest().body("invalid run identifier"),
    };

    match manager.get(run_id).await {
        Some(run)
            if owns_run(
                &run,
                &request,
                auth.as_deref().map(|v| &**v),
                store.as_ref().map(|store| store.get_ref().as_ref()),
            ) =>
        {
            HttpResponse::Ok().json(GetRunResponse {
                run_id: run.id.to_string(),
                agent: run.agent,
                status: status_name(run.status),
                response: run.response,
                error: run.error,
            })
        }
        Some(_) | None => HttpResponse::NotFound().finish(),
    }
}

#[get("/{run_id}/events")]
async fn get_run_events(
    request: HttpRequest,
    auth: Option<Data<crate::auth::TelegramAuth>>,
    store: Option<Data<Arc<SqliteConversationStore>>>,
    manager: Data<RunManager>,
    run_id: Path<String>,
) -> impl Responder {
    let run_id = match RunId::from_str(&run_id) {
        Ok(run_id) => run_id,
        Err(_) => return HttpResponse::BadRequest().body("invalid run identifier"),
    };

    let Some(run) = manager.get(run_id).await else {
        return HttpResponse::NotFound().finish();
    };
    if !owns_run(
        &run,
        &request,
        auth.as_deref().map(|v| &**v),
        store.as_ref().map(|store| store.get_ref().as_ref()),
    ) {
        return HttpResponse::NotFound().finish();
    }
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
async fn cancel_run(
    request: HttpRequest,
    auth: Option<Data<crate::auth::TelegramAuth>>,
    store: Option<Data<Arc<SqliteConversationStore>>>,
    manager: Data<RunManager>,
    run_id: Path<String>,
) -> impl Responder {
    let run_id = match RunId::from_str(&run_id) {
        Ok(run_id) => run_id,
        Err(_) => return HttpResponse::BadRequest().body("invalid run identifier"),
    };

    let Some(run) = manager.get(run_id).await else {
        return HttpResponse::NotFound().finish();
    };
    if !owns_run(
        &run,
        &request,
        auth.as_deref().map(|v| &**v),
        store.as_ref().map(|store| store.get_ref().as_ref()),
    ) {
        return HttpResponse::NotFound().finish();
    }
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
        .service(get_usage)
        .service(list_pricing)
        .service(create_pricing)
        .service(submit_run)
        .service(get_run)
        .service(get_run_events)
        .service(cancel_run)
}
