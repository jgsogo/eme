use agent_storage::FamilyRelationship;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct CreateFamilyRequest {
    pub name: String,
    pub relationship: FamilyRelationship,
}
