use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
    /// Opaque provider-specific data needed to continue a conversation.
    pub provider_metadata: Option<Value>,
}
