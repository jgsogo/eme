//! Shared API response conversion and wire-format types.

mod agent_catalog;
mod agent_list_item;
mod cancel_run_response;
mod get_run_response;
mod run_event_response;
use agent_service::{RunEvent, RunEventKind};

mod submit_run_request;
mod submit_run_response;

pub use agent_catalog::AgentCatalog;
pub use agent_list_item::AgentListItem;
pub use cancel_run_response::CancelRunResponse;
pub use get_run_response::GetRunResponse;
pub use run_event_response::RunEventResponse;
pub use submit_run_request::SubmitRunRequest;
pub use submit_run_response::SubmitRunResponse;

impl From<RunEvent> for RunEventResponse {
    fn from(event: RunEvent) -> Self {
        let occurred_at_unix_ms = event
            .occurred_at
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();

        Self {
            sequence: event.sequence,
            kind: event_kind_name(event.kind),
            iteration: event.iteration,
            tool_name: event.tool_name,
            error_category: event.error_category,
            occurred_at_unix_ms,
        }
    }
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
        RunEventKind::ModelResponded => "model_responded",
        RunEventKind::ToolExecutionStarted => "tool_execution_started",
        RunEventKind::ToolExecutionFinished => "tool_execution_finished",
        RunEventKind::ModelFailed => "model_failed",
        RunEventKind::RuntimeCancelled => "runtime_cancelled",
        RunEventKind::RuntimeTimedOut => "runtime_timed_out",
    }
}

impl AgentCatalog {
    pub fn new(agents: impl IntoIterator<Item = AgentListItem>) -> Self {
        let mut agents: Vec<_> = agents.into_iter().collect();
        agents.sort_by(|left, right| left.name.cmp(&right.name));
        Self {
            agents: agents.into(),
        }
    }

    pub fn list(&self) -> &[AgentListItem] {
        &self.agents
    }
}
