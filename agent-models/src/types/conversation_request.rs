use super::{ConversationMessage, ToolDefinition};

#[derive(Debug, Clone)]
pub struct ConversationRequest {
    pub system: Option<String>,
    pub messages: Vec<ConversationMessage>,
    pub tools: Vec<ToolDefinition>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}
