use serde::{Deserialize};
#[derive(Deserialize)]
pub(super) struct ApiMessageResponse {
    pub(super) content: Option<String>,
    pub(super) tool_calls: Option<Vec<ApiToolCallResponse>>,
}
