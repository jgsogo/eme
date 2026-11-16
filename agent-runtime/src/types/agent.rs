//! Agent identity, system instructions, tool grants, and iteration limit.

/// Configuration for an agent.
#[derive(Debug, Clone)]
pub struct Agent {
    pub id: String,
    pub system_prompt: Option<String>,
    /// Names of the tools available to this agent.
    pub tool_names: Vec<String>,
    pub max_iterations: usize,
}
