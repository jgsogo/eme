use serde::{Deserialize, Serialize};

use super::ToolCall;

/// A message in the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    User {
        content: String,
    },
    Assistant {
        content: Option<String>,
        tool_calls: Vec<ToolCall>,
    },
    Tool {
        tool_call_id: String,
        name: String,
        content: String,
        is_error: bool,
        provider_metadata: Option<serde_json::Value>,
    },
}
