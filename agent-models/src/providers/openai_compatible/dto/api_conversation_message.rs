use super::*;
use serde::{Serialize};
#[derive(Serialize)]
pub(in crate::providers::openai_compatible) struct ApiConversationMessage {
    pub(in crate::providers::openai_compatible) role: String,
    pub(in crate::providers::openai_compatible) content: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::providers::openai_compatible) name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::providers::openai_compatible) tool_calls: Option<Vec<ApiToolCall>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::providers::openai_compatible) tool_call_id: Option<String>,
}
