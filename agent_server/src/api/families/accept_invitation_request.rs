use agent_storage::FamilyRelationship;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct AcceptInvitationRequest {
    pub token: String,
    pub relationship: FamilyRelationship,
}
