use serde::{Serialize};
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FunctionCall {
    pub(super) name: String,
    pub(super) args: serde_json::Value,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) id: Option<String>,
}
