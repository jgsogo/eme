use agent_storage::{Family, FamilyMember};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct FamilyDetails {
    pub family: Family,
    pub members: Vec<FamilyMember>,
}
