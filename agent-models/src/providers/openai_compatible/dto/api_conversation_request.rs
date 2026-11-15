use super::*;
use serde::{Serialize};
#[derive(Serialize)]
pub(in crate::providers::openai_compatible) struct ApiConversationRequest {
    pub(in crate::providers::openai_compatible) model: String,
    pub(in crate::providers::openai_compatible) messages: Vec<ApiConversationMessage>,
    pub(in crate::providers::openai_compatible) tools: Option<Vec<ApiTool>>,
    pub(in crate::providers::openai_compatible) temperature: Option<f32>,
    pub(in crate::providers::openai_compatible) max_tokens: Option<u32>,
}
