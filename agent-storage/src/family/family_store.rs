//! Family identifier typeFamily persistence operations

use crate::{
    Family, FamilyId, FamilyMember, FamilyRelationship, FamilyRole, FamilyStorageError, UserId,
};
use async_trait::async_trait;

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
