use serde::Deserialize;
#[derive(Deserialize)]
pub(in crate::providers::openai_compatible) struct ApiUsage {
    pub(in crate::providers::openai_compatible) prompt_tokens: u32,
    pub(in crate::providers::openai_compatible) completion_tokens: u32,
}
