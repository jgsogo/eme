use serde::{Serialize};
#[derive(Serialize)]
pub(super) struct ApiConversationMessage {
    pub(super) role: String,
    pub(super) content: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tool_calls: Option<Vec<ApiToolCall>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tool_call_id: Option<String>,
}
