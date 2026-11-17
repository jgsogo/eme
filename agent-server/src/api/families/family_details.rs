//! Family details together with the current member list.

use agent_storage::{Family, FamilyMember};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct FamilyMemberDetails {
    #[serde(flatten)]
    pub member: FamilyMember,
    pub display_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FamilyDetails {
    pub family: Family,
    pub members: Vec<FamilyMemberDetails>,
}
