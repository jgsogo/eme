//! #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyInvitationSummary

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::{FamilyId, UserId};
use super::{FamilyInvitation, InvitationTargetIdentity};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyInvitationSummary {
    pub id: uuid::Uuid,
    pub family_id: FamilyId,
    pub inviter_id: UserId,
    pub target_identity: Option<InvitationTargetIdentity>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub accepted_by: Option<UserId>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl From<&FamilyInvitation> for FamilyInvitationSummary {
    fn from(value: &FamilyInvitation) -> Self {
        Self {
            id: value.id,
            family_id: value.family_id,
            inviter_id: value.inviter_id,
            target_identity: value.target_identity.clone(),
            created_at: value.created_at,
            expires_at: value.expires_at,
            accepted_at: value.accepted_at,
            accepted_by: value.accepted_by,
            revoked_at: value.revoked_at,
        }
    }
}
