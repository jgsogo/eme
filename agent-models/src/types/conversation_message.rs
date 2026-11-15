use serde_json::Value;
use super::ToolCall;

#[derive(Debug, Clone)]
pub enum ConversationMessage {
    User { content: String },
    Assistant { content: Option<String>, tool_calls: Vec<ToolCall> },
    Tool {
        tool_call_id: String,
        name: String,
        content: String,
        is_error: bool,
        provider_metadata: Option<Value>,
    },
}
