use super::*;
use serde::Deserialize;
#[derive(Deserialize)]
pub(in crate::providers::openai_compatible) struct ApiToolCallResponse {
    pub(in crate::providers::openai_compatible) id: String,
    pub(in crate::providers::openai_compatible) function: ApiFunctionCallResponse,
}
