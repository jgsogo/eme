use agent_storage::FamilyRole;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ChangeRoleRequest {
    pub role: FamilyRole,
}
