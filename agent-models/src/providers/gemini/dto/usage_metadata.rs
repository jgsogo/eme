use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::providers::gemini) struct UsageMetadata {
    pub(in crate::providers::gemini) prompt_token_count: u32,
    pub(in crate::providers::gemini) candidates_token_count: u32,
    #[serde(default)]
    pub(in crate::providers::gemini) cached_content_token_count: Option<u32>,
    #[serde(default)]
    pub(in crate::providers::gemini) thoughts_token_count: Option<u32>,
}
