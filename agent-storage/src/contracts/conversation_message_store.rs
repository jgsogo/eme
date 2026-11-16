//! Appends and reads ordered

use super::{StorageError, StoredMessage};
use async_trait::async_trait;
use uuid::Uuid;

/// Appends and reads ordered conversation messages.
///
/// Appends must be contiguous and start at sequence zero. Each message is
/// immutable once stored; corrections should be represented by a new message.
#[async_trait]
pub trait ConversationMessageStore: Send + Sync {
    async fn append_message(&self, message: StoredMessage) -> Result<(), StorageError>;
    async fn list_messages(
        &self,
        conversation_id: Uuid,
    ) -> Result<Vec<StoredMessage>, StorageError>;
}
