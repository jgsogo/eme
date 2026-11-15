use super::*;
use serde::{Deserialize};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::providers::gemini) struct ResponseFunctionCall {
    pub(in crate::providers::gemini) name: String,
    pub(in crate::providers::gemini) args: serde_json::Value,
    pub(in crate::providers::gemini) id: Option<String>,
}
