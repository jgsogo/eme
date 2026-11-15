use agent_runtime::{Agent, AgentRuntime};
use std::sync::Arc;

/// An agent definition paired with the runtime configured to execute it.
pub struct ResolvedAgent {
    pub agent: Agent,
    pub runtime: Arc<AgentRuntime>,
}
