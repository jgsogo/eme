//! Invitation typeInvitation operations.

use super::{
    FamilyInvitationSummary, InvitationStorageError, InvitationTargetIdentity, InvitationToken,
};
use crate::{FamilyId, FamilyMember, FamilyRelationship, UserId};
use async_trait::async_trait;
use chrono::{DateTime, Utc};

/// Invitation operations. Acceptance and membership creation must be one
/// atomic operation in every backend.
#[async_trait]
pub trait FamilyInvitationStore: Send + Sync {
    /// Creates an invitation and returns the raw token exactly once.
    async fn create_invitation(
        &self,
        family_id: FamilyId,
        inviter_id: UserId,
        target_identity: Option<InvitationTargetIdentity>,
        expires_at: DateTime<Utc>,
    ) -> Result<(FamilyInvitationSummary, InvitationToken), InvitationStorageError>;

    /// Returns metadata only. Neither hashes nor raw tokens are exposed.
    async fn list_invitations(
        &self,
        family_id: FamilyId,
    ) -> Result<Vec<FamilyInvitationSummary>, InvitationStorageError>;

    async fn revoke_invitation(
        &self,
        family_id: FamilyId,
        invitation_id: uuid::Uuid,
    ) -> Result<FamilyInvitationSummary, InvitationStorageError>;

    /// Checks token validity and identity binding without consuming the invitation.
    /// Acceptance repeats all checks atomically to protect against races.
    async fn validate_invitation(
        &self,
        token: &InvitationToken,
        accepting_identity: &InvitationTargetIdentity,
    ) -> Result<(), InvitationStorageError>;

    /// Validates the token and optional identity binding, then creates the
    /// membership and marks the invitation accepted under one transaction.
    /// Repeated acceptance by the same EME user is idempotent.
    async fn accept_invitation(
        &self,
        token: &InvitationToken,
        accepting_identity: &InvitationTargetIdentity,
        user_id: UserId,
        relationship: FamilyRelationship,
    ) -> Result<FamilyMember, InvitationStorageError>;
}

#[cfg(test)]
mod token_tests {
    use super::*;

    #[test]
    fn parses_only_fixed_length_hexadecimal_tokens() {
        let token = InvitationToken::generate();
        assert_eq!(InvitationToken::parse(token.expose()).unwrap(), token);
        assert!(InvitationToken::parse("not-a-token").is_err());
        assert!(InvitationToken::parse(&token.expose()[..63]).is_err());
        assert!(InvitationToken::parse(&format!("{}z", &token.expose()[..63])).is_err());
    }
}
