//! Payload for selecting or clearing a conversation's active family.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SelectConversationFamilyRequest {
    pub family_id: Option<String>,
}
