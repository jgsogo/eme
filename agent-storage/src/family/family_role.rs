//! Family identifier typePermission granted

use serde::{Deserialize, Serialize};

/// Permission granted to a member within one family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FamilyRole {
    Owner,
    Admin,
    Member,
}
