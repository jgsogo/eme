//! Internal state used to supervise an active or queued run.

use crate::{Run, RunEvent, RunEventKind};
use agent_runtime::{
    CancellationToken, Message, RunEvent as RuntimeEvent, RunEventKind as RuntimeEventKind,
    ToolContext,
};

pub(super) struct ManagedRun {
    pub(super) run: Run,
    pub(super) cancellation: CancellationToken,
    pub(super) events: Vec<RunEvent>,
    pub(super) history: Vec<Message>,
    pub(super) context: ToolContext,
}

impl ManagedRun {
    pub(super) fn record_runtime(&mut self, event: &RuntimeEvent) {
        let (kind, tool_name, model_id, next_model_id, model_candidates, attempted_candidates, error_category) =
            match &event.kind {
                RuntimeEventKind::ModelRequested => (RunEventKind::ModelRequested, None, None, None, None, None, None),
                RuntimeEventKind::ModelCandidatesDiscovered { model_ids } => (
                    RunEventKind::ModelCandidatesDiscovered, None, None, None, Some(model_ids.clone()), None, None,
                ),
                RuntimeEventKind::ModelCandidateAttemptStarted { model_id } => (
                    RunEventKind::ModelCandidateAttemptStarted, None, Some(model_id.clone()), None, None, None, None,
                ),
                RuntimeEventKind::ModelCandidateAttemptFailed { model_id, category } => (
                    RunEventKind::ModelCandidateAttemptFailed, None, Some(model_id.clone()), None, None, None,
                    Some(model_failure_category_name(*category)),
                ),
                RuntimeEventKind::ModelFallback { from_model_id, to_model_id, category } => (
                    RunEventKind::ModelFallback, None, Some(from_model_id.clone()), Some(to_model_id.clone()),
                    None, None, Some(model_failure_category_name(*category)),
                ),
                RuntimeEventKind::ModelSelected { model_id } => (
                    RunEventKind::ModelSelected, None, Some(model_id.clone()), None, None, None, None,
                ),
                RuntimeEventKind::ModelRoutingFailed { attempted_candidates } => (
                    RunEventKind::ModelRoutingFailed, None, None, None, None, Some(*attempted_candidates), None,
                ),
                RuntimeEventKind::ModelResponded { model_id, .. } => (
                    RunEventKind::ModelResponded, None, Some(model_id.clone()), None, None, None, None,
                ),
                RuntimeEventKind::ToolExecutionStarted { name, .. } => (
                    RunEventKind::ToolExecutionStarted, Some(name.clone()), None, None, None, None, None,
                ),
                RuntimeEventKind::ToolExecutionFinished { name, error_category, .. } => (
                    RunEventKind::ToolExecutionFinished, Some(name.clone()), None, None, None, None,
                    error_category.map(error_category_name),
                ),
                RuntimeEventKind::ModelFailed { category, .. } => (
                    RunEventKind::ModelFailed, None, None, None, None, None, Some(error_category_name(*category)),
                ),
                RuntimeEventKind::Cancelled => (RunEventKind::RuntimeCancelled, None, None, None, None, None, None),
                RuntimeEventKind::TimedOut => (RunEventKind::RuntimeTimedOut, None, None, None, None, None, None),
            };
        self.events.push(RunEvent {
            sequence: self.events.len() as u64,
            kind,
            iteration: Some(event.iteration),
            tool_name,
            model_id,
            next_model_id,
            model_candidates,
            attempted_candidates,
            error_category,
            occurred_at: std::time::SystemTime::now(),
        });
    }

    /// Records a service or lifecycle event in the shared ordered history.
    pub(super) fn record(&mut self, kind: RunEventKind) {
        self.events.push(RunEvent {
            sequence: self.events.len() as u64,
            kind,
            iteration: None,
            tool_name: None,
            model_id: None,
            next_model_id: None,
            model_candidates: None,
            attempted_candidates: None,
            error_category: None,
            occurred_at: std::time::SystemTime::now(),
        });
    }
}

fn error_category_name(category: agent_runtime::ErrorCategory) -> &'static str {
    match category {
        agent_runtime::ErrorCategory::Configuration => "configuration",
        agent_runtime::ErrorCategory::Authorization => "authorization",
        agent_runtime::ErrorCategory::Model => "model",
        agent_runtime::ErrorCategory::Tool => "tool",
        agent_runtime::ErrorCategory::Timeout => "timeout",
        agent_runtime::ErrorCategory::Cancellation => "cancellation",
    }
}


fn model_failure_category_name(category: agent_models::ModelFailureCategory) -> &'static str {
    match category {
        agent_models::ModelFailureCategory::RateLimited => "rate_limited",
        agent_models::ModelFailureCategory::QuotaExceeded => "quota_exceeded",
        agent_models::ModelFailureCategory::Transient => "transient",
        agent_models::ModelFailureCategory::CandidateUnavailable => "candidate_unavailable",
        agent_models::ModelFailureCategory::Permanent => "permanent",
    }
}
