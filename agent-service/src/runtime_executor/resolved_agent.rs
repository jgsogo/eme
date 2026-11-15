use std::sync::Arc;
use agent_runtime::{Agent, AgentRuntime};

/// An agent definition paired with the runtime configured to execute it.
pub struct ResolvedAgent {
    pub agent: Agent,
    pub runtime: Arc<AgentRuntime>,
}
