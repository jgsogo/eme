//! Message variants exchanged in an agent conversation.

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
        /// Provider-qualified model identifier that generated this assistant turn.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model_id: Option<String>,
        /// Time at which the model response was received.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        generated_at: Option<chrono::DateTime<chrono::Utc>>,
    },
    Tool {
        tool_call_id: String,
        name: String,
        content: String,
        is_error: bool,
        provider_metadata: Option<serde_json::Value>,
    },
}
