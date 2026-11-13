use async_trait::async_trait;

use crate::error::ModelError;
use crate::types::{CompletionRequest, ModelResponse};

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(&self, request: CompletionRequest) -> Result<ModelResponse, ModelError>;
}
