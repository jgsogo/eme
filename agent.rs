use serde::Deserialize;

use crate::Agent;

#[derive(Debug, Clone, Deserialize)]
pub struct AgentDefinition {
    pub name: String,
    pub description: String,
    pub model: String,
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
            system_prompt: Some(definition.instructions.clone()),
            tool_names: definition.tools.clone(),
            max_iterations: definition.max_iterations,
        }
    }
}
