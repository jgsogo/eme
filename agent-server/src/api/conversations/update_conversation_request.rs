//! Payload for renaming an existing conversation.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct UpdateConversationRequest {
    pub title: String,
}
