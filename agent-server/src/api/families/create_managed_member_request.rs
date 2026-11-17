use agent_storage::FamilyRelationship;
use chrono::NaiveDate;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct CreateManagedMemberRequest {
    pub name: String,
    pub relationship: FamilyRelationship,
    pub birthday: Option<NaiveDate>,
}
