use async_trait::async_trait;

use crate::client::LlmClient;
use crate::error::ModelError;
use crate::types::{CompletionRequest, ModelResponse};

pub struct OpenAICompatibleClient {}


impl OpenAICompatibleClient {
    pub fn new(base_url: impl Into<String>, key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {  }
    }
}

#[async_trait]
impl LlmClient for OpenAICompatibleClient {
    async fn complete(&self, request: CompletionRequest) -> Result<ModelResponse, ModelError> {
        todo!()
    }
}
