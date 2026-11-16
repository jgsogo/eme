//! Errors produced by family workflows and their storage adapters.

use agent_storage::{FamilyStorageError, InvitationStorageError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FamilyServiceError {
    #[error("family not found")]
    NotFound,
    #[error("the actor is not a member of this family")]
    NotMember,
    #[error("the actor does not have permission to perform this operation")]
    Forbidden,
    #[error("family name must not be empty")]
    EmptyName,
    #[error("the owner must transfer ownership before leaving")]
    OwnerMustTransfer,
    #[error("the target must already be a member of this family")]
    TargetNotMember,
    #[error("an owner role can only be assigned by transferring ownership")]
    OwnerRoleRequiresTransfer,
    #[error("the actor cannot change their own role")]
    CannotChangeOwnRole,
    #[error("family storage failed: {0}")]
    FamilyStorage(String),
    #[error("invitation storage failed: {0}")]
    InvitationStorage(String),
}

impl From<FamilyStorageError> for FamilyServiceError {
    fn from(error: FamilyStorageError) -> Self {
        match error {
            FamilyStorageError::FamilyNotFound(_) => Self::NotFound,
            FamilyStorageError::MembershipNotFound { .. } => Self::NotMember,
            FamilyStorageError::EmptyName => Self::EmptyName,
            FamilyStorageError::OwnerCannotLeave => Self::OwnerMustTransfer,
            FamilyStorageError::TransferTargetNotMember => Self::TargetNotMember,
            FamilyStorageError::OwnerRoleRequiresTransfer => Self::OwnerRoleRequiresTransfer,
            other => Self::FamilyStorage(other.to_string()),
        }
    }
}

impl From<InvitationStorageError> for FamilyServiceError {
    fn from(error: InvitationStorageError) -> Self {
        match error {
            InvitationStorageError::FamilyNotFound(_) => Self::NotFound,
            other => Self::InvitationStorage(other.to_string()),
        }
    }
}
