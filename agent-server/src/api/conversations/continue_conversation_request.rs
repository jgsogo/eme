//! Payload containing the next user input for a conversation.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ContinueConversationRequest {
    pub input: String,
    /// Re-submit the most recent matching user message instead of appending it again.
    #[serde(default)]
    pub retry: bool,
}
