//! Process-local family and membership store.

use super::State;
use crate::{
    Family, FamilyId, FamilyMember, FamilyRelationship, FamilyRole, FamilyStorageError,
    FamilyStore, UserId,
};
use async_trait::async_trait;
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone, Default)]
pub struct InMemoryFamilyStore {
    state: Arc<RwLock<State>>,
}

#[async_trait]
impl FamilyStore for InMemoryFamilyStore {
    async fn create_family(
        &self,
        family: Family,
        owner: FamilyMember,
    ) -> Result<(), FamilyStorageError> {
        if family.name.trim().is_empty() {
            return Err(FamilyStorageError::EmptyName);
        }
        if family.id != owner.family_id
            || family.creator_id != owner.user_id
            || owner.role != FamilyRole::Owner
        {
            return Err(FamilyStorageError::OwnerInvariant);
        }
        let mut state = self.state.write().await;
        if state.families.contains_key(&family.id) {
            return Err(FamilyStorageError::FamilyAlreadyExists(family.id));
        }
        state.families.insert(family.id, family);
        state
            .members
            .insert((owner.family_id, owner.user_id), owner);
        Ok(())
    }

    async fn list_families(&self, user_id: UserId) -> Result<Vec<Family>, FamilyStorageError> {
        let state = self.state.read().await;
        let mut result: Vec<_> = state
            .members
            .values()
            .filter(|member| member.user_id == user_id)
            .filter_map(|member| state.families.get(&member.family_id).cloned())
            .collect();
        result.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.id.to_string().cmp(&b.id.to_string()))
        });
        Ok(result)
    }

    async fn get_family(&self, family_id: FamilyId) -> Result<Option<Family>, FamilyStorageError> {
        Ok(self.state.read().await.families.get(&family_id).cloned())
    }

    async fn rename_family(
        &self,
        family_id: FamilyId,
        name: &str,
    ) -> Result<Family, FamilyStorageError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(FamilyStorageError::EmptyName);
        }
        let mut state = self.state.write().await;
        let family = state
            .families
            .get_mut(&family_id)
            .ok_or(FamilyStorageError::FamilyNotFound(family_id))?;
        family.name = name.to_owned();
        family.updated_at = Utc::now();
        Ok(family.clone())
    }

    async fn delete_family(&self, family_id: FamilyId) -> Result<(), FamilyStorageError> {
        let mut state = self.state.write().await;
        if state.families.remove(&family_id).is_none() {
            return Err(FamilyStorageError::FamilyNotFound(family_id));
        }
        state.members.retain(|(id, _), _| *id != family_id);
        Ok(())
    }

    async fn list_members(
        &self,
        family_id: FamilyId,
    ) -> Result<Vec<FamilyMember>, FamilyStorageError> {
        let state = self.state.read().await;
        if !state.families.contains_key(&family_id) {
            return Err(FamilyStorageError::FamilyNotFound(family_id));
        }
        let mut result: Vec<_> = state
            .members
            .values()
            .filter(|m| m.family_id == family_id)
            .cloned()
            .collect();
        result.sort_by_key(|m| m.joined_at);
        Ok(result)
    }

    async fn add_member(&self, member: FamilyMember) -> Result<(), FamilyStorageError> {
        if member.role == FamilyRole::Owner {
            return Err(FamilyStorageError::OwnerRoleRequiresTransfer);
        }
        let mut state = self.state.write().await;
        if !state.families.contains_key(&member.family_id) {
            return Err(FamilyStorageError::FamilyNotFound(member.family_id));
        }
        let key = (member.family_id, member.user_id);
        if state.members.contains_key(&key) {
            return Err(FamilyStorageError::MembershipAlreadyExists {
                family_id: member.family_id,
                user_id: member.user_id,
            });
        }
        state.members.insert(key, member);
        Ok(())
    }

    async fn remove_member(
        &self,
        family_id: FamilyId,
        user_id: UserId,
    ) -> Result<(), FamilyStorageError> {
        self.remove_non_owner(family_id, user_id).await
    }

    async fn leave_family(
        &self,
        family_id: FamilyId,
        user_id: UserId,
    ) -> Result<(), FamilyStorageError> {
        self.remove_non_owner(family_id, user_id).await
    }

    async fn change_member_role(
        &self,
        family_id: FamilyId,
        user_id: UserId,
        role: FamilyRole,
    ) -> Result<FamilyMember, FamilyStorageError> {
        if role == FamilyRole::Owner {
            return Err(FamilyStorageError::OwnerRoleRequiresTransfer);
        }
        let mut state = self.state.write().await;
        let member = state
            .members
            .get_mut(&(family_id, user_id))
            .ok_or(FamilyStorageError::MembershipNotFound { family_id, user_id })?;
        if member.role == FamilyRole::Owner {
            return Err(FamilyStorageError::OwnerRoleRequiresTransfer);
        }
        member.role = role;
        Ok(member.clone())
    }

    async fn change_member_relationship(
        &self,
        family_id: FamilyId,
        user_id: UserId,
        relationship: FamilyRelationship,
    ) -> Result<FamilyMember, FamilyStorageError> {
        let mut state = self.state.write().await;
        let member = state
            .members
            .get_mut(&(family_id, user_id))
            .ok_or(FamilyStorageError::MembershipNotFound { family_id, user_id })?;
        member.relationship = relationship;
        Ok(member.clone())
    }

    async fn transfer_ownership(
        &self,
        family_id: FamilyId,
        current_owner: UserId,
        new_owner: UserId,
    ) -> Result<(), FamilyStorageError> {
        let mut state = self.state.write().await;
        if !state.families.contains_key(&family_id) {
            return Err(FamilyStorageError::FamilyNotFound(family_id));
        }
        let current_key = (family_id, current_owner);
        let new_key = (family_id, new_owner);
        let current =
            state
                .members
                .get(&current_key)
                .ok_or(FamilyStorageError::MembershipNotFound {
                    family_id,
                    user_id: current_owner,
                })?;
        if current.role != FamilyRole::Owner {
            return Err(FamilyStorageError::NotCurrentOwner);
        }
        if current_owner == new_owner {
            return Ok(());
        }
        if !state.members.contains_key(&new_key) {
            return Err(FamilyStorageError::TransferTargetNotMember);
        }
        state.members.get_mut(&current_key).unwrap().role = FamilyRole::Admin;
        state.members.get_mut(&new_key).unwrap().role = FamilyRole::Owner;
        Ok(())
    }
}

impl InMemoryFamilyStore {
    async fn remove_non_owner(
        &self,
        family_id: FamilyId,
        user_id: UserId,
    ) -> Result<(), FamilyStorageError> {
        let mut state = self.state.write().await;
        if !state.families.contains_key(&family_id) {
            return Err(FamilyStorageError::FamilyNotFound(family_id));
        }
        let key = (family_id, user_id);
        let member = state
            .members
            .get(&key)
            .ok_or(FamilyStorageError::MembershipNotFound { family_id, user_id })?;
        if member.role == FamilyRole::Owner {
            return Err(FamilyStorageError::OwnerCannotLeave);
        }
        state.members.remove(&key);
        Ok(())
    }
}
