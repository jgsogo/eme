use agent_storage::FamilyRelationship;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ChangeRelationshipRequest {
    pub relationship: FamilyRelationship,
}
