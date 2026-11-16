use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ContinueConversationRequest {
    pub input: String,
}
