//! Family invitation domain types and storage contract.
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{FamilyId, FamilyMember, FamilyRelationship, UserId};

/// External identity an invitation is optionally restricted to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvitationTargetIdentity {
    pub provider: String,
    pub subject: String,
}

impl InvitationTargetIdentity {
    pub fn new(provider: impl Into<String>, subject: impl Into<String>) -> Result<Self, InvitationStorageError> {
        let provider = provider.into();
        let subject = subject.into();
        if provider.trim().is_empty() || subject.trim().is_empty() {
            return Err(InvitationStorageError::EmptyTargetIdentity);
        }
        Ok(Self { provider, subject })
    }
}

/// Raw bearer token. Its Debug implementation deliberately never reveals it.
#[derive(Clone, PartialEq, Eq)]
pub struct InvitationToken(String);

impl InvitationToken {
    pub(crate) fn generate() -> Self {
        // Two independent UUIDv4 values provide 244 bits of CSPRNG-backed entropy.
        Self(format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple()))
    }

    pub(crate) fn hash(&self) -> String {
        format!("{:x}", Sha256::digest(self.0.as_bytes()))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for InvitationToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("InvitationToken([REDACTED])")
    }
}

/// Persisted invitation. The token hash is intentionally private and is never
/// included in list responses or serialized API-facing summaries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum InvitationStorageError {
    #[error("family not found: {0}")]
    FamilyNotFound(FamilyId),
    #[error("invitation not found")]
    NotFound,
    #[error("invitation has expired")]
    Expired,
    #[error("invitation has been revoked")]
    Revoked,
    #[error("invitation has already been accepted")]
    AlreadyAccepted,
    #[error("invitation target identity does not match the accepting identity")]
    IdentityMismatch,
    #[error("invitation expiry must be in the future")]
    InvalidExpiry,
    #[error("target identity provider and subject must not be empty")]
    EmptyTargetIdentity,
    #[error("inviter is not a member of the family")]
    InviterNotMember,
    #[error("membership already exists; invitation cannot create a duplicate membership")]
    MembershipAlreadyExists,
    #[error("membership creation failed")]
    MembershipCreationFailed,
}

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
