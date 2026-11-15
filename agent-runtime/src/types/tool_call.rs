use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A tool invocation requested by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
    pub provider_metadata: Option<Value>,
}
