//! Payload for creating a family and assigning the creator's relationship.

use agent_storage::FamilyRelationship;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct CreateFamilyRequest {
    pub name: String,
    pub relationship: FamilyRelationship,
}
