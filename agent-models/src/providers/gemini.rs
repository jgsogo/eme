use async_trait::async_trait;

use crate::client::LlmClient;

use crate::error::ModelError;
use crate::types::{CompletionRequest, ModelResponse};
pub struct GeminiClient {}

impl GeminiClient {
    pub fn new(key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {}
    }
}

#[async_trait]
impl LlmClient for GeminiClient {
    async fn complete(&self, request: CompletionRequest) -> Result<ModelResponse, ModelError> {
        todo!()
    }
}
