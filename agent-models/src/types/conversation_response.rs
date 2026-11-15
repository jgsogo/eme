use super::{TokenUsage, ToolCall};

#[derive(Debug, Clone)]
pub struct ConversationResponse {
    pub model: String,
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Option<TokenUsage>,
}
