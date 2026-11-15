use agent_service::{RunEvent, RunEventKind};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct SubmitRunRequest {
    pub agent: String,
    pub input: String,
}

#[derive(Debug, Serialize)]
pub struct SubmitRunResponse {
    pub run_id: String,
    pub status: &'static str,
}

#[derive(Debug, Serialize)]
pub struct GetRunResponse {
    pub run_id: String,
    pub agent: String,
    pub status: &'static str,
    pub response: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RunEventResponse {
    pub sequence: u64,
    pub kind: &'static str,
    pub iteration: Option<usize>,
    pub tool_name: Option<String>,
    pub error_category: Option<&'static str>,
    pub occurred_at_unix_ms: u128,
}

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

#[derive(Debug, Serialize)]
pub struct CancelRunResponse {
    pub run_id: String,
    pub status: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentListItem {
    pub name: String,
    pub description: String,
    pub model: String,
    pub tools: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AgentCatalog {
    agents: std::sync::Arc<[AgentListItem]>,
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
