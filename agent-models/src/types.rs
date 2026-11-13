#[derive(Debug, Clone)]
pub struct CompletionRequest {}

impl CompletionRequest {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {}
    }
}

#[derive(Debug, Clone)]
pub struct ModelResponse {
    pub content: String,
}
