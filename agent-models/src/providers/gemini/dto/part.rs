use serde::{Serialize};
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Part {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) text: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) function_call: Option<FunctionCall>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) function_response: Option<FunctionResponse>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) thought_signature: Option<String>,
}
