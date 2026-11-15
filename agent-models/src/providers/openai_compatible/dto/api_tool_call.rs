use super::*;
use serde::Serialize;
#[derive(Serialize)]
pub(in crate::providers::openai_compatible) struct ApiToolCall {
    pub(in crate::providers::openai_compatible) id: String,
    #[serde(rename = "type")]
    pub(in crate::providers::openai_compatible) kind: String,
    pub(in crate::providers::openai_compatible) function: ApiFunctionCall,
}
