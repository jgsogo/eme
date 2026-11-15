use super::*;
use serde::{Deserialize};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PartResponse {
    pub(super) text: Option<String>,
    pub(super) function_call: Option<ResponseFunctionCall>,
    pub(super) thought_signature: Option<String>,
}
