use serde::Deserialize;
#[derive(Deserialize)]
pub(in crate::providers::openai_compatible) struct ApiFunctionCallResponse {
    pub(in crate::providers::openai_compatible) name: String,
    pub(in crate::providers::openai_compatible) arguments: String,
}
