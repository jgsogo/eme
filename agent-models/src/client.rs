use async_trait::async_trait;

use crate::error::ModelError;
use crate::types::{ConversationRequest, ConversationResponse};

#[async_trait]
pub trait ConversationClient: Send + Sync {
    async fn complete_conversation(
        &self,
        request: ConversationRequest,
    ) -> Result<ConversationResponse, ModelError>;
}
