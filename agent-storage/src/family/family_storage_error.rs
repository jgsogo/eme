//! #[derive(Debug, Error, Clone, PartialEq, Eq)]

use crate::{FamilyId, UserId};
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FamilyStorageError {
    #[error("family not found: {0}")]
    FamilyNotFound(FamilyId),
    #[error("family already exists: {0}")]
    FamilyAlreadyExists(FamilyId),
    #[error("membership not found for user {user_id} in family {family_id}")]
    MembershipNotFound {
        family_id: FamilyId,
        user_id: UserId,
    },
    #[error("membership already exists for user {user_id} in family {family_id}")]
    MembershipAlreadyExists {
        family_id: FamilyId,
        user_id: UserId,
    },
    #[error("family name must not be empty")]
    EmptyName,
    #[error("family must have exactly one owner")]
    OwnerInvariant,
    #[error("the owner cannot be removed or leave before transferring ownership")]
    OwnerCannotLeave,
    #[error("ownership transfer target must already be a family member")]
    TransferTargetNotMember,
    #[error("the acting user is not the current owner")]
    NotCurrentOwner,
    #[error("owner role can only be changed by transferring ownership")]
    OwnerRoleRequiresTransfer,
    #[error("family storage backend failed: {0}")]
    Backend(String),
}
