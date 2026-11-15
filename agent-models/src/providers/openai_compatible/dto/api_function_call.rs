use serde::Serialize;
#[derive(Serialize)]
pub(in crate::providers::openai_compatible) struct ApiFunctionCall {
    pub(in crate::providers::openai_compatible) name: String,
    pub(in crate::providers::openai_compatible) arguments: String,
}
