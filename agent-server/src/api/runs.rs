//! Run submission, inspection, cancellation, and event routes.

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use agent_metering::{
    CostEstimate, CostStatus, ModelPricing, UsageRecord, calculate_cost, summarize_usage,
};

use super::types::{
    CancelRunResponse, GetRunResponse, RunEventResponse, SubmitRunRequest, SubmitRunResponse,
};
use actix_web::{
    HttpRequest, HttpResponse, Responder, Scope, get, post,
    http::header,
    web::{self, Bytes, Data, Json, Path},
};
use agent_runtime::ToolContext;
use agent_service::Run;
use agent_service::{
    CancelRunError, CancelRunOutcome, RunId, RunManager, RunStatus, SubmitRunError,
};
use agent_storage::{
    ConversationId, FileUserIdentityStore, SqliteConversationStore, UserIdentityStore,
};
use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc;
use tokio_stream::{StreamExt, wrappers::ReceiverStream};

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
            conversation_id: run.conversation_id.map(|id| id.to_string()),
            input: run.input,
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
    let visible_run_ids = manager
        .list()
        .await
        .into_iter()
        .filter(|run| {
            owns_run(
                run,
                &request,
                auth.as_deref().map(|value| &**value),
                store.as_ref().map(|value| value.get_ref().as_ref()),
            )
        })
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
    for record in records
        .into_iter()
        .filter(|record| visible_run_ids.contains(&record.run_id))
    {
        let cost = if let Some(reported) = &record.reported_cost {
            Some(CostEstimate::from_reported(reported))
        } else {
            let key = (record.provider.clone(), record.model.clone());
            if !pricing_cache.contains_key(&key) {
                match manager.pricing_for(&key.0, &key.1).await {
                    Ok(pricing) => {
                        pricing_cache.insert(key.clone(), pricing);
                    }
                    Err(error) => {
                        tracing::error!(provider = %key.0, model = %key.1, %error, "failed to load model pricing");
                        pricing_cache.insert(key.clone(), Vec::new());
                    }
                }
            }
            pricing_cache.get(&key).and_then(|versions| {
                versions
                    .iter()
                    .find(|pricing| pricing.applies_to(&record))
                    .and_then(|pricing| calculate_cost(&record, pricing).ok())
                    .filter(|cost| cost.status != CostStatus::Unavailable)
            })
        };

        let mut item = match serde_json::to_value(&record) {
            Ok(value) => value,
            Err(error) => {
                tracing::error!(%error, "failed to serialize usage record");
                return HttpResponse::InternalServerError()
                    .body("could not serialize usage records");
            }
        };
        item["cost"] = serde_json::to_value(cost).unwrap_or(serde_json::Value::Null);
        response.push(item);
    }

    HttpResponse::Ok().json(response)
}

fn estimate_record_cost(record: &UsageRecord, pricing: &[ModelPricing]) -> Option<CostEstimate> {
    if let Some(reported) = &record.reported_cost {
        return Some(CostEstimate::from_reported(reported));
    }
    pricing
        .iter()
        .find(|price| price.applies_to(record))
        .and_then(|price| calculate_cost(record, price).ok())
        .filter(|cost| cost.status != CostStatus::Unavailable)
}

#[get("/{run_id}/usage")]
async fn get_run_usage(
    path: Path<String>,
    request: HttpRequest,
    auth: Option<Data<crate::auth::TelegramAuth>>,
    store: Option<Data<Arc<SqliteConversationStore>>>,
    manager: Data<RunManager>,
) -> impl Responder {
    let run_id = match RunId::from_str(&path) {
        Ok(run_id) => run_id,
        Err(_) => return HttpResponse::BadRequest().body("invalid run ID"),
    };
    let run = manager
        .list()
        .await
        .into_iter()
        .find(|run| run.id == run_id);
    let Some(run) = run else {
        return HttpResponse::NotFound().finish();
    };
    if !owns_run(
        &run,
        &request,
        auth.as_deref().map(|value| &**value),
        store.as_ref().map(|value| value.get_ref().as_ref()),
    ) {
        return HttpResponse::Forbidden().finish();
    }

    let records = match manager.usage_records(Some(run_id)).await {
        Ok(records) => records,
        Err(error) => {
            tracing::error!(%run_id, %error, "failed to load run usage records");
            return HttpResponse::InternalServerError().body("could not load run usage");
        }
    };
    let pricing = match manager.pricing_catalog().await {
        Ok(pricing) => pricing,
        Err(error) => {
            tracing::error!(%run_id, %error, "failed to load pricing catalogue");
            return HttpResponse::InternalServerError().body("could not load pricing catalogue");
        }
    };
    let summary = summarize_usage(&records, &pricing);
    let invocations = records
        .iter()
        .map(|record| {
            let mut item = serde_json::to_value(record).unwrap_or(serde_json::Value::Null);
            item["cost"] = serde_json::to_value(estimate_record_cost(record, &pricing))
                .unwrap_or(serde_json::Value::Null);
            item
        })
        .collect::<Vec<_>>();

    HttpResponse::Ok().json(serde_json::json!({
        "run_id": run_id.to_string(),
        "summary": summary,
        "invocations": invocations
    }))
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
    if auth
        .as_ref()
        .and_then(|auth| auth.owner_for_request(&request))
        .is_none()
    {
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
                conversation_id: run.conversation_id.map(|id| id.to_string()),
                input: run.input,
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

#[get("/{run_id}/events/stream")]
async fn stream_run_events(
    request: HttpRequest,
    auth: Option<Data<crate::auth::TelegramAuth>>,
    store: Option<Data<Arc<SqliteConversationStore>>>,
    manager: Data<RunManager>,
    run_id: Path<String>,
) -> HttpResponse {
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

    // Subscribe before taking the snapshot. Events arriving in between are
    // represented in both and de-duplicated by their per-run sequence number.
    let mut live_events = manager.subscribe_events();
    let replay = manager.events(run_id).await.unwrap_or_default();
    let last_event_id = request
        .headers()
        .get("Last-Event-ID")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    let (sender, receiver) = mpsc::channel::<Bytes>(32);
    let manager = manager.clone();

    actix_web::rt::spawn(async move {
        let mut last_sequence = last_event_id;
        for event in replay {
            if last_sequence.is_some_and(|last| event.sequence <= last) {
                continue;
            }
            let Some((sequence, terminal)) = send_sse_event(&sender, event).await else {
                return;
            };
            last_sequence = Some(sequence);
            if terminal {
                return;
            }
        }

        if manager
            .get(run_id)
            .await
            .is_some_and(|run| !matches!(run.status, RunStatus::Queued | RunStatus::Running))
        {
            return;
        }

        let mut heartbeat = tokio::time::interval(Duration::from_secs(15));
        loop {
            tokio::select! {
                received = live_events.recv() => {
                    match received {
                        Ok((event_run_id, event)) if event_run_id == run_id => {
                            if last_sequence.is_some_and(|last| event.sequence <= last) {
                                continue;
                            }
                            let Some((sequence, terminal)) = send_sse_event(&sender, event).await else {
                                return;
                            };
                            last_sequence = Some(sequence);
                            if terminal {
                                return;
                            }
                        }
                        Ok(_) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            // Recover from a slow consumer using ordered run history.
                            for event in manager.events(run_id).await.unwrap_or_default() {
                                if last_sequence.is_some_and(|last| event.sequence <= last) {
                                    continue;
                                }
                                let Some((sequence, terminal)) = send_sse_event(&sender, event).await else {
                                    return;
                                };
                                last_sequence = Some(sequence);
                                if terminal {
                                    return;
                                }
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                    }
                }
                _ = heartbeat.tick() => {
                    if sender.send(Bytes::from_static(b": keep-alive\\n\\n")).await.is_err() {
                        return;
                    }
                }
            }
        }
    });

    HttpResponse::Ok()
        .insert_header((header::CONTENT_TYPE, "text/event-stream"))
        .insert_header((header::CACHE_CONTROL, "no-cache"))
        .insert_header(("X-Accel-Buffering", "no"))
        .streaming(ReceiverStream::new(receiver).map(Ok::<Bytes, actix_web::Error>))
}

async fn send_sse_event(
    sender: &mpsc::Sender<Bytes>,
    event: agent_service::RunEvent,
) -> Option<(u64, bool)> {
    let response = RunEventResponse::from(event);
    let sequence = response.sequence;
    let terminal = matches!(
        response.kind,
        "completed" | "failed" | "cancelled" | "timed_out" | "max_iterations_reached"
    );
    let payload = serde_json::to_string(&response).ok()?;
    let frame = format!("id: {sequence}\ndata: {payload}\n\n");
    sender.send(Bytes::from(frame)).await.ok()?;
    Some((sequence, terminal))
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
        .service(get_run_usage)
        .service(submit_run)
        .service(get_run)
        .service(get_run_events)
        .service(stream_run_events)
        .service(cancel_run)
}
