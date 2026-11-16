use serde::Deserialize;
use agent_storage::FamilyRelationship;

#[derive(Debug, Deserialize)]
pub struct CreateFamilyRequest {
    pub name: String,
    pub relationship: FamilyRelationship,
}
