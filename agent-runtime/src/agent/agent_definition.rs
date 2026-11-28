//! Deserialized agent configuration and conversion to the runtime's execution model.

use agent_models::ModelCapability;
use serde::Deserialize;

use crate::Agent;

#[derive(Debug, Clone, Deserialize)]
pub struct AgentDefinition {
    pub name: String,
    pub description: String,
    pub capabilities: Vec<ModelCapability>,
    pub instructions: String,

    #[serde(default)]
    pub tools: Vec<String>,

    /// Agent identifiers this agent is explicitly allowed to delegate to.
    #[serde(default)]
    pub allowed_delegates: Vec<String>,

    /// Maximum number of delegation edges below this agent.
    #[serde(default = "default_max_delegation_depth")]
    pub max_delegation_depth: usize,

    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,
}

fn default_max_iterations() -> usize {
    10
}

fn default_max_delegation_depth() -> usize {
    3
}

impl From<&AgentDefinition> for Agent {
    fn from(definition: &AgentDefinition) -> Self {
        let mut tool_names = definition.tools.clone();
        if !definition.allowed_delegates.is_empty()
            && !tool_names.iter().any(|name| name == "delegate_agent")
        {
            tool_names.push("delegate_agent".into());
        }

        Self {
            id: definition.name.clone(),
            capabilities: definition.capabilities.clone(),
            system_prompt: Some(definition.instructions.clone()),
            tool_names,
            max_iterations: definition.max_iterations,
        }
    }
}
