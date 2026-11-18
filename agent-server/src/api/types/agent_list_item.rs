//! Public summary of one configured agent.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct AgentListItem {
    pub name: String,
    pub description: String,
    pub capabilities: Vec<agent_models::ModelCapability>,
    pub tools: Vec<String>,
}
