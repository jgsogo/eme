use serde::Deserialize;

/// OpenAI-compatible prompt token breakdown.
#[derive(Deserialize)]
pub(in crate::providers::openai_compatible) struct ApiPromptTokensDetails {
    #[serde(default)]
    pub(in crate::providers::openai_compatible) cached_tokens: Option<u32>,
}
