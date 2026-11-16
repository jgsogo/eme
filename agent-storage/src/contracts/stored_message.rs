//! A message in a conversation

use agent_runtime::Message;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A message in a conversation, optionally associated with the run that added it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub conversation_id: Uuid,
    pub run_id: Option<Uuid>,
    /// Zero-based, contiguous sequence number within the conversation.
    pub sequence: u64,
    pub message: Message,
    pub created_at: DateTime<Utc>,
}
