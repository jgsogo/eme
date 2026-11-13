#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub system: Option<String>,
    pub prompt: String,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}

impl CompletionRequest {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            system: None,
            prompt: prompt.into(),
            temperature: Some(0.7),
            max_tokens: Some(1024),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModelResponse {
    pub model: String,
    pub content: String,
    pub usage: Option<TokenUsage>,
}

#[derive(Debug, Clone)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}
