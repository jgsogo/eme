//! Conversation metadata and model-visible message history.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use agent_runtime::Message;
use crate::FamilyId;
use super::ConversationId;

/// A conversation and its complete model-visible message history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: ConversationId,
    /// Stable external identity, for example `telegram:<user_id>`.
    pub owner: String,
    pub agent: String,
    /// User-editable display name. Defaults when reading older conversation files.
    #[serde(default = "default_conversation_title")]
    pub title: String,
    /// Family explicitly selected for this conversation; absent in legacy records.
    #[serde(default)]
    pub selected_family_id: Option<FamilyId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub messages: Vec<Message>,
}
