//! #[derive(Debug, Error, Clone, PartialEq, Eq)]

use crate::FamilyId;
use thiserror::Error;

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
    #[error("invitation storage backend failed: {0}")]
    Backend(String),
}
