use super::*;
use serde::{Serialize};
#[derive(Serialize)]
pub(super) struct ApiConversationRequest {
    pub(super) model: String,
    pub(super) messages: Vec<ApiConversationMessage>,
    pub(super) tools: Option<Vec<ApiTool>>,
    pub(super) temperature: Option<f32>,
    pub(super) max_tokens: Option<u32>,
}
