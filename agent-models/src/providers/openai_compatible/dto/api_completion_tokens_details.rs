use serde::Deserialize;

/// OpenAI-compatible completion token breakdown.
#[derive(Deserialize)]
pub(in crate::providers::openai_compatible) struct ApiCompletionTokensDetails {
    #[serde(default)]
    pub(in crate::providers::openai_compatible) reasoning_tokens: Option<u32>,
}
