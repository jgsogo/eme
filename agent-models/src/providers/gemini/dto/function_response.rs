use super::*;
use serde::{Serialize};
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FunctionResponse {
    pub(super) name: String,
    pub(super) response: serde_json::Value,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) id: Option<String>,
}
