use serde::{Serialize};
#[derive(Serialize)]
pub(super) struct ApiToolCall {
    pub(super) id: String,
    #[serde(rename = "type")]
    pub(super) kind: String,
    pub(super) function: ApiFunctionCall,
}
