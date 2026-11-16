//! Model output for one turn, including optional text and requested tool calls.

use super::ToolCall;

/// Response received from the model.
#[derive(Debug, Clone)]
pub struct ModelTurn {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
}
