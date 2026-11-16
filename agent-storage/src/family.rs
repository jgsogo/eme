//! Family and membership domain types and storage contract.
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::UserId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FamilyId(Uuid);

impl FamilyId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}
impl Default for FamilyId {
    fn default() -> Self {
        Self::new()
    }
}
impl std::fmt::Display for FamilyId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Permission granted to a member within one family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FamilyRole {
    Owner,
    Admin,
    Member,
}

/// Describes a member's relationship within one family, not their permissions.
/// Add variants as the product vocabulary evolves; do not infer tree constraints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FamilyRelationship {
    Father,
    Mother,
    Son,
    Daughter,
    Grandfather,
    Grandmother,
    Partner,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Family {
    pub id: FamilyId,
    pub name: String,
    /// Historical attribution; this user need not remain a member.
    pub creator_id: UserId,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyMember {
    pub family_id: FamilyId,
    pub user_id: UserId,
    pub role: FamilyRole,
    pub relationship: FamilyRelationship,
    pub joined_at: DateTime<Utc>,
}

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

/// Family persistence operations. Methods that affect ownership or family
/// lifecycle are atomic; implementations must preserve exactly one owner.
#[async_trait]
pub trait FamilyStore: Send + Sync {
    async fn create_family(
        &self,
        family: Family,
        owner: FamilyMember,
    ) -> Result<(), FamilyStorageError>;
    async fn list_families(&self, user_id: UserId) -> Result<Vec<Family>, FamilyStorageError>;
    async fn get_family(&self, family_id: FamilyId) -> Result<Option<Family>, FamilyStorageError>;
    async fn rename_family(
        &self,
        family_id: FamilyId,
        name: &str,
    ) -> Result<Family, FamilyStorageError>;
    async fn delete_family(&self, family_id: FamilyId) -> Result<(), FamilyStorageError>;
    async fn list_members(
        &self,
        family_id: FamilyId,
    ) -> Result<Vec<FamilyMember>, FamilyStorageError>;
    async fn add_member(&self, member: FamilyMember) -> Result<(), FamilyStorageError>;
    async fn remove_member(
        &self,
        family_id: FamilyId,
        user_id: UserId,
    ) -> Result<(), FamilyStorageError>;
    async fn leave_family(
        &self,
        family_id: FamilyId,
        user_id: UserId,
    ) -> Result<(), FamilyStorageError>;
    async fn change_member_role(
        &self,
        family_id: FamilyId,
        user_id: UserId,
        role: FamilyRole,
    ) -> Result<FamilyMember, FamilyStorageError>;
    async fn change_member_relationship(
        &self,
        family_id: FamilyId,
        user_id: UserId,
        relationship: FamilyRelationship,
    ) -> Result<FamilyMember, FamilyStorageError>;
    async fn transfer_ownership(
        &self,
        family_id: FamilyId,
        current_owner: UserId,
        new_owner: UserId,
    ) -> Result<(), FamilyStorageError>;
}
