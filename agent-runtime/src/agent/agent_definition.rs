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

    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,
}

fn default_max_iterations() -> usize {
    10
}

impl From<&AgentDefinition> for Agent {
    fn from(definition: &AgentDefinition) -> Self {
        Self {
            id: definition.name.clone(),
            capabilities: definition.capabilities.clone(),
            system_prompt: Some(definition.instructions.clone()),
            tool_names: definition.tools.clone(),
            max_iterations: definition.max_iterations,
        }
    }
}
