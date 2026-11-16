use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct UpdateConversationRequest {
    pub title: String,
}
