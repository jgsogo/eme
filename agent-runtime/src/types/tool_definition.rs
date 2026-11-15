use serde::Deserialize;
use serde_json::Value;

/// Description of a tool exposed to the model.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    /// JSON Schema describing the tool arguments.
    pub parameters: Value,
}
