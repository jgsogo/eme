use super::*;
use serde::Deserialize;
#[derive(Deserialize)]
pub(in crate::providers::openai_compatible) struct ApiMessageResponse {
    pub(in crate::providers::openai_compatible) content: Option<String>,
    pub(in crate::providers::openai_compatible) tool_calls: Option<Vec<ApiToolCallResponse>>,
}
