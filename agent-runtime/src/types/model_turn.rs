//! Model output for one turn, including optional text and requested tool calls.

use super::ToolCall;
use agent_models::TokenUsage;

/// Response received from the model.
#[derive(Debug, Clone)]
pub struct ModelTurn {
    /// Usage returned by the provider, if available.
    pub usage: Option<TokenUsage>,
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
}
