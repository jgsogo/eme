use serde::{Deserialize};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ResponseFunctionCall {
    pub(super) name: String,
    pub(super) args: serde_json::Value,
    pub(super) id: Option<String>,
}
