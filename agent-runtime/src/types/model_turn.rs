use super::ToolCall;

/// Response received from the model.
#[derive(Debug, Clone)]
pub struct ModelTurn {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
}
