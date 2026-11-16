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
        let (kind, tool_name, error_category) = match &event.kind {
            RuntimeEventKind::ModelRequested => (RunEventKind::ModelRequested, None, None),
            RuntimeEventKind::ModelResponded { .. } => (RunEventKind::ModelResponded, None, None),
            RuntimeEventKind::ToolExecutionStarted { name, .. } => {
                (RunEventKind::ToolExecutionStarted, Some(name.clone()), None)
            }
            RuntimeEventKind::ToolExecutionFinished {
                name,
                error_category,
                ..
            } => (
                RunEventKind::ToolExecutionFinished,
                Some(name.clone()),
                error_category.map(error_category_name),
            ),
            RuntimeEventKind::ModelFailed { category, .. } => (
                RunEventKind::ModelFailed,
                None,
                Some(error_category_name(*category)),
            ),
            RuntimeEventKind::Cancelled => (RunEventKind::RuntimeCancelled, None, None),
            RuntimeEventKind::TimedOut => (RunEventKind::RuntimeTimedOut, None, None),
        };
        self.events.push(RunEvent {
            sequence: self.events.len() as u64,
            kind,
            iteration: Some(event.iteration),
            tool_name,
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
