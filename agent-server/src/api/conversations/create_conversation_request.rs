//! Payload for creating a conversation with a configured agent.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct CreateConversationRequest {
    pub agent: String,
    /// Optional family context to attach at creation time.
    #[serde(default)]
    pub family_id: Option<String>,
}
