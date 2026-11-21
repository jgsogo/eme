/// Provider-reported token counts normalized across model APIs.
#[derive(Debug, Clone)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    /// Cached input tokens when the provider exposes this dimension.
    pub cached_input_tokens: Option<u32>,
    /// Reasoning/thinking tokens when the provider exposes this dimension.
    pub reasoning_tokens: Option<u32>,
}
