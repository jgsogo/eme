use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AgentDefinition {
    pub name: String,
    pub description: String,
    pub model: String,
    pub instructions: String,
    #[serde(default)]
    pub tools: Vec<String>,
}