//! Contract implemented by model-provider conversation clients.

use async_trait::async_trait;

use crate::{ConversationRequest, ConversationResponse, ModelError};

#[async_trait]
pub trait ConversationClient: Send + Sync {
    async fn complete_conversation(
        &self,
        request: ConversationRequest,
    ) -> Result<ConversationResponse, ModelError>;
}
