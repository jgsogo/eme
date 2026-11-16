//! Membership record connecting a user to a family.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::{FamilyId, FamilyRelationship, FamilyRole, UserId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyMember {
    pub family_id: FamilyId,
    pub user_id: UserId,
    pub role: FamilyRole,
    pub relationship: FamilyRelationship,
    pub joined_at: DateTime<Utc>,
}
