use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SelectConversationFamilyRequest {
    pub family_id: Option<String>,
}
