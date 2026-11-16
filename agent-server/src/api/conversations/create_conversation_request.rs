//! Payload for creating a conversation with a configured agent.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct CreateConversationRequest {
    pub agent: String,
}
