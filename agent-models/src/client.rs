use async_trait::async_trait;

use crate::error::ModelError;
use crate::types::{CompletionRequest, ConversationRequest, ConversationResponse, ModelResponse};

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(&self, request: CompletionRequest) -> Result<ModelResponse, ModelError>;
}

#[async_trait]
pub trait ConversationClient: Send + Sync {
    async fn complete_conversation(
        &self,
        request: ConversationRequest,
    ) -> Result<ConversationResponse, ModelError>;
}
