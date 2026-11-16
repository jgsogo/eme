//! Payload for changing a family member's role.

use agent_storage::FamilyRole;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ChangeRoleRequest {
    pub role: FamilyRole,
}
