use super::*;
use serde::{Deserialize};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::providers::gemini) struct PartResponse {
    pub(in crate::providers::gemini) text: Option<String>,
    pub(in crate::providers::gemini) function_call: Option<ResponseFunctionCall>,
    pub(in crate::providers::gemini) thought_signature: Option<String>,
}
