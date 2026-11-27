//! Internal state used to supervise an active or queued run.

use crate::{Run, RunEvent, RunEventKind, RunId};
use agent_runtime::{
    CancellationToken, Message, RunEvent as RuntimeEvent, RunEventKind as RuntimeEventKind,
    ToolContext,
};
use tokio::sync::broadcast;

pub(super) struct ManagedRun {
    pub(super) run: Run,
    pub(super) cancellation: CancellationToken,
    pub(super) events: Vec<RunEvent>,
    pub(super) history: Vec<Message>,
    pub(super) context: ToolContext,
    pub(super) persisted_message_count: Option<u64>,
    pub(super) event_sender: broadcast::Sender<(RunId, RunEvent)>,
}

impl ManagedRun {
    pub(super) fn record_runtime(&mut self, event: &RuntimeEvent) {
        let (
            kind,
            tool_name,
            model_id,
            next_model_id,
            model_candidates,
            attempted_candidates,
            error_category,
        ) = match &event.kind {
            RuntimeEventKind::ModelUsageRecorded { .. } => return,
            RuntimeEventKind::ModelRequested => (
                RunEventKind::ModelRequested,
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            RuntimeEventKind::ModelCandidatesDiscovered { model_ids } => (
                RunEventKind::ModelCandidatesDiscovered,
                None,
                None,
                None,
                Some(model_ids.clone()),
                None,
                None,
            ),
            RuntimeEventKind::ModelCandidateAttemptStarted { model_id } => (
                RunEventKind::ModelCandidateAttemptStarted,
                None,
                Some(model_id.clone()),
                None,
                None,
                None,
                None,
            ),
            RuntimeEventKind::ModelCandidateAttemptFailed { model_id, category } => (
                RunEventKind::ModelCandidateAttemptFailed,
                None,
                Some(model_id.clone()),
                None,
                None,
                None,
                Some(model_failure_category_name(*category)),
            ),
            RuntimeEventKind::ModelFallback {
                from_model_id,
                to_model_id,
                category,
            } => (
                RunEventKind::ModelFallback,
                None,
                Some(from_model_id.clone()),
                Some(to_model_id.clone()),
                None,
                None,
                Some(model_failure_category_name(*category)),
            ),
            RuntimeEventKind::ModelSelected { model_id } => (
                RunEventKind::ModelSelected,
                None,
                Some(model_id.clone()),
                None,
                None,
                None,
                None,
            ),
            RuntimeEventKind::ModelRoutingFailed {
                attempted_candidates,
            } => (
                RunEventKind::ModelRoutingFailed,
                None,
                None,
                None,
                None,
                Some(*attempted_candidates),
                None,
            ),
            RuntimeEventKind::ModelResponded { model_id, .. } => (
                RunEventKind::ModelResponded,
                None,
                Some(model_id.clone()),
                None,
                None,
                None,
                None,
            ),
            RuntimeEventKind::ToolExecutionStarted { name, .. } => (
                RunEventKind::ToolExecutionStarted,
                Some(name.clone()),
                None,
                None,
                None,
                None,
                None,
            ),
            RuntimeEventKind::ToolExecutionFinished {
                name,
                error_category,
                ..
            } => (
                RunEventKind::ToolExecutionFinished,
                Some(name.clone()),
                None,
                None,
                None,
                None,
                error_category.map(error_category_name),
            ),
            RuntimeEventKind::HallucinatedToolInvocation { tool_name } => (
                RunEventKind::HallucinatedToolInvocation,
                Some(tool_name.clone()),
                None,
                None,
                None,
                None,
                None,
            ),
            RuntimeEventKind::ModelFailed { category, .. } => (
                RunEventKind::ModelFailed,
                None,
                None,
                None,
                None,
                None,
                Some(error_category_name(*category)),
            ),
            RuntimeEventKind::Cancelled => (
                RunEventKind::RuntimeCancelled,
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            RuntimeEventKind::TimedOut => (
                RunEventKind::RuntimeTimedOut,
                None,
                None,
                None,
                None,
                None,
                None,
            ),
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
        self.publish_last_event();
    }

    /// Builds a lifecycle event without exposing it to subscribers yet.
    /// Terminal events use this to ensure the transcript is durable first.
    pub(super) fn next_event(&self, kind: RunEventKind) -> RunEvent {
        RunEvent {
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
        }
    }

    /// Adds an event to the ordered history and broadcasts it to subscribers.
    pub(super) fn publish_event(&mut self, event: RunEvent) {
        debug_assert_eq!(event.sequence, self.events.len() as u64);
        self.events.push(event.clone());
        let _ = self.event_sender.send((self.run.id, event));
    }

    fn publish_last_event(&self) {
        if let Some(event) = self.events.last() {
            let _ = self.event_sender.send((self.run.id, event.clone()));
        }
    }

    /// Records a service or lifecycle event in the shared ordered history.
    pub(super) fn record(&mut self, kind: RunEventKind) {
        let event = self.next_event(kind);
        self.publish_event(event);
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

fn model_failure_category_name(category: agent_runtime::ModelFailureCategory) -> &'static str {
    match category {
        agent_runtime::ModelFailureCategory::RateLimited => "rate_limited",
        agent_runtime::ModelFailureCategory::QuotaExceeded => "quota_exceeded",
        agent_runtime::ModelFailureCategory::Transient => "transient",
        agent_runtime::ModelFailureCategory::CandidateUnavailable => "candidate_unavailable",
        agent_runtime::ModelFailureCategory::Permanent => "permanent",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RunStatus;

    #[test]
    fn pending_terminal_event_is_not_visible_until_published() {
        let (event_sender, mut receiver) = broadcast::channel(8);
        let mut managed = ManagedRun {
            run: Run::new("assistant", "hello"),
            cancellation: CancellationToken::new(),
            events: Vec::new(),
            history: Vec::new(),
            context: ToolContext::default(),
            persisted_message_count: None,
            event_sender,
        };
        managed.run.mark_running().unwrap();
        let event = managed.next_event(RunEventKind::Completed);

        assert_eq!(managed.run.status, RunStatus::Running);
        assert!(managed.events.is_empty());
        assert!(matches!(
            receiver.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));

        managed.publish_event(event);

        assert_eq!(managed.events.len(), 1);
        assert_eq!(managed.events[0].kind, RunEventKind::Completed);
        assert_eq!(receiver.try_recv().unwrap().1.kind, RunEventKind::Completed);
    }
}
