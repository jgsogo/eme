//! Invitation typePersisted invitation.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::{FamilyId, UserId};
use super::InvitationTargetIdentity;

/// Persisted invitation. The token hash is intentionally private and is never
/// included in list responses or serialized API-facing summaries.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyInvitation {
    pub id: uuid::Uuid,
    pub family_id: FamilyId,
    pub inviter_id: UserId,
    pub target_identity: Option<InvitationTargetIdentity>,
    #[serde(skip_serializing)]
    pub(crate) token_hash: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub accepted_by: Option<UserId>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl std::fmt::Debug for FamilyInvitation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FamilyInvitation")
            .field("id", &self.id)
            .field("family_id", &self.family_id)
            .field("inviter_id", &self.inviter_id)
            .field("target_identity", &self.target_identity)
            .field("token_hash", &"[REDACTED]")
            .field("created_at", &self.created_at)
            .field("expires_at", &self.expires_at)
            .field("accepted_at", &self.accepted_at)
            .field("accepted_by", &self.accepted_by)
            .field("revoked_at", &self.revoked_at)
            .finish()
    }
}
