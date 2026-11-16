//! Payload containing the next user input for a conversation.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ContinueConversationRequest {
    pub input: String,
}
