use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct CreateConversationRequest {
    pub agent: String,
}
