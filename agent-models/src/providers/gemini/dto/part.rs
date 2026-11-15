use super::*;
use serde::Serialize;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::providers::gemini) struct Part {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::providers::gemini) text: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::providers::gemini) function_call: Option<FunctionCall>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::providers::gemini) function_response: Option<FunctionResponse>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::providers::gemini) thought_signature: Option<String>,
}
