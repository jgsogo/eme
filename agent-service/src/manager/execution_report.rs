use super::ExecutionOutcome;
use agent_runtime::{Message, RunEvent as RuntimeEvent};

/// Runtime outcome and the events emitted while executing it.
#[derive(Debug, Clone)]
pub struct ExecutionReport {
    pub outcome: ExecutionOutcome,
    pub runtime_events: Vec<RuntimeEvent>,
    /// Complete model-visible history after execution, when supported.
    pub messages: Option<Vec<Message>>,
}
