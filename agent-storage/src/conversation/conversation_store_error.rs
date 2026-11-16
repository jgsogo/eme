//! Failures returned by conversation persistence.

use super::ConversationId;
use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConversationStoreError {
    #[error("conversation storage I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("conversation serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("conversation {0} was not found")]
    NotFound(ConversationId),
    #[error("conversation {conversation_id} does not belong to owner '{owner}'")]
    OwnerMismatch {
        conversation_id: ConversationId,
        owner: String,
    },
    #[error("conversation owner must not be empty")]
    EmptyOwner,
    #[error("agent name must not be empty")]
    EmptyAgent,
    #[error("conversation title must not be empty")]
    EmptyTitle,
}
