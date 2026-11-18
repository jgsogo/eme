//! Agent identity, system instructions, tool grants, and iteration limit.

/// Configuration for an agent.
#[derive(Debug, Clone)]
pub struct Agent {
    pub id: String,
    /// Model capabilities that must be available for every model invocation.
    pub capabilities: Vec<agent_models::ModelCapability>,
    pub system_prompt: Option<String>,
    /// Names of the tools available to this agent.
    pub tool_names: Vec<String>,
    pub max_iterations: usize,
}
