//! Coordinates run admission, bounded execution, cancellation, and durable history.

use agent_metering::{ModelPricing, UsageRecord};

use super::{
    CancelRunError, CancelRunOutcome, ExecutionOutcome, ExecutionReport, ManagedRun, RunExecutor,
    RunManagerConfigError, SubmitRunError,
};
use crate::{Run, RunEvent, RunEventKind, RunId, RunStatus};
use agent_runtime::{CancellationToken, EventSink, Message, RunEventKind as RuntimeRunEventKind, ToolContext};
use agent_storage::{ExecutionStore, StoredMessage, StoredRun, StoredRunEvent, StoredRunStatus, UsageStore};
use chrono::{DateTime, Utc};
use std::{
    collections::HashMap,
    str::FromStr,
    sync::{Arc, RwLock},
    time::SystemTime,
};
use tokio::sync::{Mutex, mpsc};
use tracing::{error, info};
use uuid::Uuid;

/// Coordinates run admission, in-memory state, and bounded execution.
#[derive(Clone)]
pub struct RunManager {
    runs: Arc<Mutex<HashMap<RunId, ManagedRun>>>,
    sender: mpsc::Sender<RunId>,
    persistence: Arc<RwLock<Option<Arc<dyn ExecutionStore>>>>,
}

impl RunManager {
    /// Creates an in-memory manager. Use `with_storage` to enable durability.
    pub fn new(
        executor: Arc<dyn RunExecutor>,
        max_concurrent_runs: usize,
        max_queued_runs: usize,
    ) -> Result<Self, RunManagerConfigError> {
        if max_concurrent_runs == 0 {
            return Err(RunManagerConfigError::ZeroConcurrency);
        }
        if max_queued_runs == 0 {
            return Err(RunManagerConfigError::ZeroQueueCapacity);
        }

        let runs = Arc::new(Mutex::new(HashMap::<RunId, ManagedRun>::new()));
        let persistence: Arc<RwLock<Option<Arc<dyn ExecutionStore>>>> = Arc::new(RwLock::new(None));
        let (sender, receiver) = mpsc::channel(max_queued_runs);
        let receiver = Arc::new(Mutex::new(receiver));

        for _ in 0..max_concurrent_runs {
            let runs = Arc::clone(&runs);
            let receiver = Arc::clone(&receiver);
            let executor = Arc::clone(&executor);
            let persistence = Arc::clone(&persistence);

            tokio::spawn(async move {
                loop {
                    let run_id = {
                        let mut receiver = receiver.lock().await;
                        receiver.recv().await
                    };
                    let Some(run_id) = run_id else { break };

                    let (run, cancellation, history, context, history_len) = {
                        let mut runs = runs.lock().await;
                        let Some(entry) = runs.get_mut(&run_id) else {
                            continue;
                        };
                        if entry.run.mark_running().is_err() {
                            continue;
                        }
                        entry.record(RunEventKind::Started);
                        (
                            entry.run.clone(),
                            entry.cancellation.clone(),
                            entry.history.clone(),
                            entry.context.clone(),
                            entry.history.len(),
                        )
                    };

                    if let Some(storage) = current_storage(&persistence) {
                        if let Err(storage_error) = persist_snapshot_and_event(
                            storage.as_ref(),
                            &run,
                            &RunEvent {
                                sequence: 1,
                                kind: RunEventKind::Started,
                                iteration: None,
                                tool_name: None,
                                model_id: None,
                                next_model_id: None,
                                model_candidates: None,
                                attempted_candidates: None,
                                error_category: None,
                                occurred_at: SystemTime::now(),
                            },
                        )
                        .await
                        {
                            error!(run_id = %run_id, %storage_error, "failed to persist run start");
                        }
                    }

                    info!(run_id = %run.id, agent = %run.agent, "agent run started");
                    let (event_sender, mut event_receiver) = mpsc::unbounded_channel();
                    let event_sink: Arc<dyn EventSink> =
                        Arc::new(RuntimeEventChannel(event_sender));
                    let execution = executor.execute_with_event_sink(
                        run.id,
                        run.agent.clone(),
                        run.input.clone(),
                        history,
                        context,
                        cancellation,
                        event_sink,
                    );
                    tokio::pin!(execution);
                    let mut events_open = true;
                    let result = loop {
                        tokio::select! {
                            runtime_event = event_receiver.recv(), if events_open => {
                                match runtime_event {
                                    Some(event) => persist_runtime_event(
                                        &runs, &persistence, run_id, &event
                                    ).await,
                                    None => events_open = false,
                                }
                            }
                            result = &mut execution => break result,
                        }
                    };
                    // Drain events emitted immediately before the executor returned.
                    while let Ok(event) = event_receiver.try_recv() {
                        persist_runtime_event(&runs, &persistence, run_id, &event).await;
                    }
                    // Executors using the default adapter may only return a batch.
                    if let Ok(report) = &result {
                        for event in &report.runtime_events {
                            persist_runtime_event(&runs, &persistence, run_id, event).await;
                        }
                    }

                    let persisted = {
                        let mut runs = runs.lock().await;
                        if let Some(entry) = runs.get_mut(&run_id) {
                            if entry.run.status != RunStatus::Running {
                                None
                            } else {
                                if let Ok(report) = &result {
                                    if let Some(messages) = &report.messages {
                                        entry.history = messages.clone();
                                    }
                                }
                                let event = match result {
                                    Ok(ExecutionReport {
                                        outcome: ExecutionOutcome::Completed(response),
                                        ..
                                    }) => {
                                        let _ = entry.run.complete(response);
                                        RunEventKind::Completed
                                    }
                                    Ok(ExecutionReport {
                                        outcome: ExecutionOutcome::Cancelled,
                                        ..
                                    }) => {
                                        let _ = entry.run.cancel();
                                        RunEventKind::Cancelled
                                    }
                                    Ok(ExecutionReport {
                                        outcome: ExecutionOutcome::TimedOut,
                                        ..
                                    }) => {
                                        let _ = entry.run.time_out();
                                        RunEventKind::TimedOut
                                    }
                                    Ok(ExecutionReport {
                                        outcome: ExecutionOutcome::MaxIterationsReached,
                                        ..
                                    }) => {
                                        let _ = entry.run.reach_max_iterations();
                                        RunEventKind::MaxIterationsReached
                                    }
                                    Ok(ExecutionReport {
                                        outcome: ExecutionOutcome::Failed(error),
                                        ..
                                    }) => {
                                        let _ = entry.run.fail(error);
                                        RunEventKind::Failed
                                    }
                                    Err(error) => {
                                        let _ = entry.run.fail(error);
                                        RunEventKind::Failed
                                    }
                                };
                                entry.record(event);
                                info!(run_id = %run_id, agent = %entry.run.agent, status = ?entry.run.status, "agent run finished");
                                Some((
                                    entry.run.clone(),
                                    entry
                                        .events
                                        .last()
                                        .cloned()
                                        .expect("terminal event was recorded"),
                                    entry.history.clone(),
                                    entry.persisted_message_count,
                                ))
                            }
                        } else {
                            None
                        }
                    };

                    if let (
                        Some(storage),
                        Some((snapshot, terminal_event, messages, message_count)),
                    ) = (current_storage(&persistence), persisted)
                    {
                        if let Err(storage_error) =
                            storage.update_run(to_stored_run(&snapshot)).await
                        {
                            error!(run_id = %run_id, %storage_error, "failed to persist terminal run state");
                        }
                        if let Err(storage_error) = storage
                            .append_event(to_stored_event(run_id, &terminal_event))
                            .await
                        {
                            error!(run_id = %run_id, sequence = terminal_event.sequence, %storage_error, "failed to persist terminal run event");
                        }
                        // Runtime results carry the complete transcript. The initial
                        // history and submitted user message are already durable.
                        if let (Some(mut next_sequence), Some(conversation_id)) =
                            (message_count, snapshot.conversation_id)
                        {
                            let suffix = messages.into_iter().skip(history_len + 1);
                            for message in suffix {
                                let stored = StoredMessage {
                                    conversation_id,
                                    run_id: Some(run_id.as_uuid()),
                                    sequence: next_sequence,
                                    message,
                                    created_at: Utc::now(),
                                };
                                match storage.append_message(stored).await {
                                    Ok(()) => next_sequence += 1,
                                    Err(storage_error) => {
                                        error!(run_id = %run_id, %storage_error, "failed to persist conversation message");
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            });
        }

        Ok(Self {
            runs,
            sender,
            persistence,
        })
    }

    /// Enables durable run, event, and conversation-message persistence.
    pub fn with_storage(self, storage: Arc<dyn ExecutionStore>) -> Self {
        // Workers share this slot, so attaching storage immediately after
        // construction is safe before the manager is handed to request handlers.
        // No work should be submitted before this builder is called.
        let mut guard = self
            .persistence
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = Some(storage);
        drop(guard);
        self
    }

    pub async fn submit(
        &self,
        agent: impl Into<String>,
        input: impl Into<String>,
        history: Vec<Message>,
        mut context: ToolContext,
    ) -> Result<RunId, SubmitRunError> {
        // Reserve queue capacity before touching durable state. This avoids
        // leaving a persisted queued run behind when admission is rejected.
        let permit = self
            .sender
            .try_reserve()
            .map_err(|_| SubmitRunError::QueueFull)?;
        let mut run = Run::new(agent, input);
        let run_id = run.id;
        let storage = current_storage(&self.persistence);
        let conversation_id = if storage.is_some() {
            context
                .conversation_id
                .as_deref()
                .map(Uuid::from_str)
                .transpose()
                .map_err(|error| {
                    SubmitRunError::Storage(format!("invalid conversation identifier: {error}"))
                })?
        } else {
            None
        };
        run.conversation_id = conversation_id;
        context.run_id = Some(run_id.as_uuid());
        let mut persisted_message_count = None;
        if let Some(storage) = storage {
            if let Some(conversation_id) = conversation_id {
                let existing = storage
                    .list_messages(conversation_id)
                    .await
                    .map_err(|error| SubmitRunError::Storage(error.to_string()))?;
                if existing.len() != history.len() {
                    return Err(SubmitRunError::Storage(format!(
                        "conversation history mismatch: storage has {} messages but caller supplied {}",
                        existing.len(),
                        history.len()
                    )));
                }
                // Compare the exact structured transcript, not only its length.
                for (stored, supplied) in existing.iter().zip(&history) {
                    let left = serde_json::to_value(&stored.message)
                        .map_err(|error| SubmitRunError::Storage(error.to_string()))?;
                    let right = serde_json::to_value(supplied)
                        .map_err(|error| SubmitRunError::Storage(error.to_string()))?;
                    if left != right {
                        return Err(SubmitRunError::Storage(
                            "conversation history differs from durable history; reload before continuing".into()
                        ));
                    }
                }
                persisted_message_count = Some(existing.len() as u64 + 1);
            }

            storage
                .insert_run(to_stored_run(&run))
                .await
                .map_err(|error| SubmitRunError::Storage(error.to_string()))?;
            storage
                .append_event(to_stored_event(run_id, &queued_event()))
                .await
                .map_err(|error| SubmitRunError::Storage(error.to_string()))?;
            if let Some(conversation_id) = conversation_id {
                storage
                    .append_message(StoredMessage {
                        conversation_id,
                        run_id: Some(run_id.as_uuid()),
                        sequence: persisted_message_count.unwrap() - 1,
                        message: Message::User {
                            content: run.input.clone(),
                        },
                        created_at: Utc::now(),
                    })
                    .await
                    .map_err(|error| SubmitRunError::Storage(error.to_string()))?;
            }
        }

        let initial_event = queued_event();
        self.runs.lock().await.insert(
            run_id,
            ManagedRun {
                run,
                cancellation: CancellationToken::new(),
                events: vec![initial_event],
                history,
                context,
                persisted_message_count,
            },
        );
        permit.send(run_id);
        info!(run_id = %run_id, "agent conversation run queued");
        Ok(run_id)
    }

    pub async fn cancel(&self, run_id: RunId) -> Result<CancelRunOutcome, CancelRunError> {
        let (outcome, snapshot, event) = {
            let mut runs = self.runs.lock().await;
            let entry = runs.get_mut(&run_id).ok_or(CancelRunError::NotFound)?;
            let outcome = match entry.run.status {
                RunStatus::Queued => {
                    entry.cancellation.cancel();
                    entry.run.cancel().expect("queued run can be cancelled");
                    entry.record(RunEventKind::Cancelled);
                    CancelRunOutcome::CancelledBeforeStart
                }
                RunStatus::Running => {
                    if entry.cancellation.is_cancelled() {
                        return Ok(CancelRunOutcome::AlreadyRequested);
                    }
                    entry.cancellation.cancel();
                    return Ok(CancelRunOutcome::Requested);
                }
                _ => return Ok(CancelRunOutcome::AlreadyFinished),
            };
            (
                outcome,
                entry.run.clone(),
                entry.events.last().cloned().unwrap(),
            )
        };
        if let Some(storage) = current_storage(&self.persistence) {
            if let Err(error) =
                persist_snapshot_and_event(storage.as_ref(), &snapshot, &event).await
            {
                error!(run_id = %run_id, %error, "failed to persist queued-run cancellation");
            }
        }
        Ok(outcome)
    }

    pub async fn list(&self) -> Vec<Run> {
        let mut by_id = HashMap::new();
        if let Some(storage) = current_storage(&self.persistence) {
            match storage.list_runs(None).await {
                Ok(stored) => {
                    for run in stored {
                        by_id.insert(run.id, from_stored_run(run));
                    }
                }
                Err(error) => error!(%error, "failed to load persisted runs"),
            }
        }
        for entry in self.runs.lock().await.values() {
            by_id.insert(entry.run.id.as_uuid(), entry.run.clone());
        }
        let mut result: Vec<_> = by_id.into_values().collect();
        result.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        result
    }

    pub async fn events(&self, run_id: RunId) -> Option<Vec<RunEvent>> {
        if let Some(events) = self
            .runs
            .lock()
            .await
            .get(&run_id)
            .map(|entry| entry.events.clone())
        {
            return Some(events);
        }
        let storage = current_storage(&self.persistence)?;
        match storage.list_events(run_id.as_uuid(), None).await {
            Ok(events) => Some(events.into_iter().map(from_stored_event).collect()),
            Err(error) => {
                error!(run_id = %run_id, %error, "failed to load persisted run events");
                None
            }
        }
    }

    pub async fn get(&self, run_id: RunId) -> Option<Run> {
        if let Some(run) = self
            .runs
            .lock()
            .await
            .get(&run_id)
            .map(|entry| entry.run.clone())
        {
            return Some(run);
        }
        let storage = current_storage(&self.persistence)?;
        match storage.get_run(run_id.as_uuid()).await {
            Ok(Some(run)) => Some(from_stored_run(run)),
            Ok(None) => None,
            Err(error) => {
                error!(run_id = %run_id, %error, "failed to load persisted run");
                None
            }
        }
    }

    /// Lists durable invocation usage, optionally restricted to one run.
    pub async fn usage_records(&self, run_id: Option<RunId>) -> Result<Vec<UsageRecord>, String> {
        let Some(storage) = current_storage(&self.persistence) else {
            return Ok(Vec::new());
        };
        storage.list_usage(run_id.map(|id| id.as_uuid())).await.map_err(|error| error.to_string())
    }

    /// Returns immutable pricing versions registered for one provider/model pair.
    pub async fn pricing_for(&self, provider: &str, model: &str) -> Result<Vec<ModelPricing>, String> {
        let Some(storage) = current_storage(&self.persistence) else {
            return Ok(Vec::new());
        };
        storage.list_pricing(provider, model).await.map_err(|error| error.to_string())
    }

    pub async fn history(&self, run_id: RunId) -> Option<Vec<Message>> {
        if let Some(history) = self
            .runs
            .lock()
            .await
            .get(&run_id)
            .map(|entry| entry.history.clone())
        {
            return Some(history);
        }
        let storage = current_storage(&self.persistence)?;
        let run = storage.get_run(run_id.as_uuid()).await.ok()??;
        let conversation_id = run.conversation_id?;
        storage
            .list_messages(conversation_id)
            .await
            .ok()
            .map(|messages| {
                messages
                    .into_iter()
                    .map(|message| message.message)
                    .collect()
            })
    }
}

struct RuntimeEventChannel(mpsc::UnboundedSender<agent_runtime::RunEvent>);

impl EventSink for RuntimeEventChannel {
    fn emit(&self, event: &agent_runtime::RunEvent) {
        let _ = self.0.send(event.clone());
    }
}

async fn persist_runtime_event(
    runs: &Arc<Mutex<HashMap<RunId, ManagedRun>>>,
    persistence: &RwLock<Option<Arc<dyn ExecutionStore>>>,
    run_id: RunId,
    runtime_event: &agent_runtime::RunEvent,
) {
    if let RuntimeRunEventKind::ModelUsageRecorded { record } = &runtime_event.kind {
        if let Some(storage) = current_storage(persistence) {
            if let Err(error) = storage.append_usage(record.clone()).await {
                error!(run_id = %run_id, invocation_id = %record.invocation_id, %error, "failed to persist model usage record");
            }
        }
        return;
    }
    let event = {
        let mut runs = runs.lock().await;
        let Some(entry) = runs.get_mut(&run_id) else { return; };
        entry.record_runtime(runtime_event);
        entry.events.last().cloned()
    };
    if let (Some(storage), Some(event)) = (current_storage(persistence), event) {
        if let Err(error) = storage.append_event(to_stored_event(run_id, &event)).await {
            error!(run_id = %run_id, sequence = event.sequence, %error, "failed to persist live runtime event");
        }
    }
}

fn current_storage(
    slot: &RwLock<Option<Arc<dyn ExecutionStore>>>,
) -> Option<Arc<dyn ExecutionStore>> {
    slot.read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

fn queued_event() -> RunEvent {
    RunEvent {
        sequence: 0,
        kind: RunEventKind::Queued,
        iteration: None,
        tool_name: None,
        model_id: None,
        next_model_id: None,
        model_candidates: None,
        attempted_candidates: None,
        error_category: None,
        occurred_at: SystemTime::now(),
    }
}

async fn persist_snapshot_and_event(
    storage: &dyn ExecutionStore,
    run: &Run,
    event: &RunEvent,
) -> Result<(), agent_storage::StorageError> {
    storage.update_run(to_stored_run(run)).await?;
    storage.append_event(to_stored_event(run.id, event)).await
}

fn to_stored_run(run: &Run) -> StoredRun {
    StoredRun {
        id: run.id.as_uuid(),
        conversation_id: run.conversation_id,
        agent: run.agent.clone(),
        input: run.input.clone(),
        status: match run.status {
            RunStatus::Queued => StoredRunStatus::Queued,
            RunStatus::Running => StoredRunStatus::Running,
            RunStatus::Completed => StoredRunStatus::Completed,
            RunStatus::Failed => StoredRunStatus::Failed,
            RunStatus::Cancelled => StoredRunStatus::Cancelled,
            RunStatus::TimedOut => StoredRunStatus::TimedOut,
            RunStatus::MaxIterationsReached => StoredRunStatus::MaxIterationsReached,
        },
        created_at: DateTime::<Utc>::from(run.created_at),
        started_at: run.started_at.map(DateTime::<Utc>::from),
        finished_at: run.finished_at.map(DateTime::<Utc>::from),
        response: run.response.clone(),
        error: run.error.clone(),
    }
}

fn from_stored_run(run: StoredRun) -> Run {
    Run {
        id: RunId::from_uuid(run.id),
        agent: run.agent,
        input: run.input,
        conversation_id: run.conversation_id,
        status: match run.status {
            StoredRunStatus::Queued => RunStatus::Queued,
            StoredRunStatus::Running => RunStatus::Running,
            StoredRunStatus::Completed => RunStatus::Completed,
            StoredRunStatus::Failed => RunStatus::Failed,
            StoredRunStatus::Cancelled => RunStatus::Cancelled,
            StoredRunStatus::TimedOut => RunStatus::TimedOut,
            StoredRunStatus::MaxIterationsReached => RunStatus::MaxIterationsReached,
        },
        created_at: run.created_at.into(),
        started_at: run.started_at.map(Into::into),
        finished_at: run.finished_at.map(Into::into),
        response: run.response,
        error: run.error,
    }
}

fn to_stored_event(run_id: RunId, event: &RunEvent) -> StoredRunEvent {
    StoredRunEvent {
        run_id: run_id.as_uuid(),
        sequence: event.sequence,
        kind: event_kind_name(event.kind).to_owned(),
        iteration: event.iteration,
        tool_name: event.tool_name.clone(),
        model_id: event.model_id.clone(),
        next_model_id: event.next_model_id.clone(),
        model_candidates: event.model_candidates.clone(),
        attempted_candidates: event.attempted_candidates,
        error_category: event.error_category.map(str::to_owned),
        occurred_at: DateTime::<Utc>::from(event.occurred_at),
    }
}

fn from_stored_event(event: StoredRunEvent) -> RunEvent {
    RunEvent {
        sequence: event.sequence,
        kind: parse_event_kind(&event.kind).unwrap_or(RunEventKind::Failed),
        iteration: event.iteration,
        tool_name: event.tool_name,
        model_id: event.model_id,
        next_model_id: event.next_model_id,
        model_candidates: event.model_candidates,
        attempted_candidates: event.attempted_candidates,
        error_category: event.error_category.as_deref().and_then(static_category),
        occurred_at: event.occurred_at.into(),
    }
}

fn static_category(value: &str) -> Option<&'static str> {
    Some(match value {
        "configuration" => "configuration",
        "authorization" => "authorization",
        "model" => "model",
        "tool" => "tool",
        "timeout" => "timeout",
        "cancellation" => "cancellation",
        "rate_limited" => "rate_limited",
        "quota_exceeded" => "quota_exceeded",
        "transient" => "transient",
        "candidate_unavailable" => "candidate_unavailable",
        "permanent" => "permanent",
        _ => return None,
    })
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
        RunEventKind::ModelCandidatesDiscovered => "model_candidates_discovered",
        RunEventKind::ModelCandidateAttemptStarted => "model_candidate_attempt_started",
        RunEventKind::ModelCandidateAttemptFailed => "model_candidate_attempt_failed",
        RunEventKind::ModelFallback => "model_fallback",
        RunEventKind::ModelSelected => "model_selected",
        RunEventKind::ModelRoutingFailed => "model_routing_failed",
        RunEventKind::ModelResponded => "model_responded",
        RunEventKind::ToolExecutionStarted => "tool_execution_started",
        RunEventKind::ToolExecutionFinished => "tool_execution_finished",
        RunEventKind::ModelFailed => "model_failed",
        RunEventKind::RuntimeCancelled => "runtime_cancelled",
        RunEventKind::RuntimeTimedOut => "runtime_timed_out",
    }
}

fn parse_event_kind(value: &str) -> Option<RunEventKind> {
    Some(match value {
        "queued" => RunEventKind::Queued,
        "started" => RunEventKind::Started,
        "completed" => RunEventKind::Completed,
        "failed" => RunEventKind::Failed,
        "cancelled" => RunEventKind::Cancelled,
        "timed_out" => RunEventKind::TimedOut,
        "max_iterations_reached" => RunEventKind::MaxIterationsReached,
        "model_requested" => RunEventKind::ModelRequested,
        "model_candidates_discovered" => RunEventKind::ModelCandidatesDiscovered,
        "model_candidate_attempt_started" => RunEventKind::ModelCandidateAttemptStarted,
        "model_candidate_attempt_failed" => RunEventKind::ModelCandidateAttemptFailed,
        "model_fallback" => RunEventKind::ModelFallback,
        "model_selected" => RunEventKind::ModelSelected,
        "model_routing_failed" => RunEventKind::ModelRoutingFailed,
        "model_responded" => RunEventKind::ModelResponded,
        "tool_execution_started" => RunEventKind::ToolExecutionStarted,
        "tool_execution_finished" => RunEventKind::ToolExecutionFinished,
        "model_failed" => RunEventKind::ModelFailed,
        "runtime_cancelled" => RunEventKind::RuntimeCancelled,
        "runtime_timed_out" => RunEventKind::RuntimeTimedOut,
        _ => return None,
    })
}
