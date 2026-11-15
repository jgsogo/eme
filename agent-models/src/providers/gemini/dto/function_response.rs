use super::*;
use serde::Serialize;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::providers::gemini) struct FunctionResponse {
    pub(in crate::providers::gemini) name: String,
    pub(in crate::providers::gemini) response: serde_json::Value,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::providers::gemini) id: Option<String>,
}
