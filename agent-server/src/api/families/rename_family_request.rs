use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct RenameFamilyRequest {
    pub name: String,
}
