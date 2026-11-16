//! In-memory family and membership storage implementation.
use std::{collections::HashMap, sync::Arc};
use async_trait::async_trait;
use tokio::sync::RwLock;
use chrono::Utc;
use crate::{
    Family, FamilyId, FamilyMember, FamilyRelationship, FamilyRole, FamilyStorageError,
    FamilyStore, UserId,
};

#[derive(Default)]
struct State {
    families: HashMap<FamilyId, Family>,
    members: HashMap<(FamilyId, UserId), FamilyMember>,
}

#[derive(Clone, Default)]
pub struct InMemoryFamilyStore { state: Arc<RwLock<State>> }

#[async_trait]
impl FamilyStore for InMemoryFamilyStore {
    async fn create_family(&self, family: Family, owner: FamilyMember) -> Result<(), FamilyStorageError> {
        if family.name.trim().is_empty() { return Err(FamilyStorageError::EmptyName); }
        if family.id != owner.family_id || family.creator_id != owner.user_id || owner.role != FamilyRole::Owner {
            return Err(FamilyStorageError::OwnerInvariant);
        }
        let mut state = self.state.write().await;
        if state.families.contains_key(&family.id) {
            return Err(FamilyStorageError::FamilyAlreadyExists(family.id));
        }
        state.families.insert(family.id, family);
        state.members.insert((owner.family_id, owner.user_id), owner);
        Ok(())
    }

    async fn list_families(&self, user_id: UserId) -> Result<Vec<Family>, FamilyStorageError> {
        let state = self.state.read().await;
        let mut result: Vec<_> = state.members.values()
            .filter(|member| member.user_id == user_id)
            .filter_map(|member| state.families.get(&member.family_id).cloned())
            .collect();
        result.sort_by(|a,b| a.created_at.cmp(&b.created_at).then_with(|| a.id.to_string().cmp(&b.id.to_string())));
        Ok(result)
    }

    async fn get_family(&self, family_id: FamilyId) -> Result<Option<Family>, FamilyStorageError> {
        Ok(self.state.read().await.families.get(&family_id).cloned())
    }

    async fn rename_family(&self, family_id: FamilyId, name: &str) -> Result<Family, FamilyStorageError> {
        let name = name.trim();
        if name.is_empty() { return Err(FamilyStorageError::EmptyName); }
        let mut state = self.state.write().await;
        let family = state.families.get_mut(&family_id).ok_or(FamilyStorageError::FamilyNotFound(family_id))?;
        family.name = name.to_owned();
        family.updated_at = Utc::now();
        Ok(family.clone())
    }

    async fn delete_family(&self, family_id: FamilyId) -> Result<(), FamilyStorageError> {
        let mut state = self.state.write().await;
        if state.families.remove(&family_id).is_none() { return Err(FamilyStorageError::FamilyNotFound(family_id)); }
        state.members.retain(|(id, _), _| *id != family_id);
        Ok(())
    }

    async fn list_members(&self, family_id: FamilyId) -> Result<Vec<FamilyMember>, FamilyStorageError> {
        let state = self.state.read().await;
        if !state.families.contains_key(&family_id) { return Err(FamilyStorageError::FamilyNotFound(family_id)); }
        let mut result: Vec<_> = state.members.values().filter(|m| m.family_id == family_id).cloned().collect();
        result.sort_by_key(|m| m.joined_at);
        Ok(result)
    }

    async fn add_member(&self, member: FamilyMember) -> Result<(), FamilyStorageError> {
        if member.role == FamilyRole::Owner { return Err(FamilyStorageError::OwnerRoleRequiresTransfer); }
        let mut state = self.state.write().await;
        if !state.families.contains_key(&member.family_id) { return Err(FamilyStorageError::FamilyNotFound(member.family_id)); }
        let key = (member.family_id, member.user_id);
        if state.members.contains_key(&key) {
            return Err(FamilyStorageError::MembershipAlreadyExists { family_id: member.family_id, user_id: member.user_id });
        }
        state.members.insert(key, member);
        Ok(())
    }

    async fn remove_member(&self, family_id: FamilyId, user_id: UserId) -> Result<(), FamilyStorageError> {
        self.remove_non_owner(family_id, user_id).await
    }

    async fn leave_family(&self, family_id: FamilyId, user_id: UserId) -> Result<(), FamilyStorageError> {
        self.remove_non_owner(family_id, user_id).await
    }

    async fn change_member_role(&self, family_id: FamilyId, user_id: UserId, role: FamilyRole) -> Result<FamilyMember, FamilyStorageError> {
        if role == FamilyRole::Owner { return Err(FamilyStorageError::OwnerRoleRequiresTransfer); }
        let mut state = self.state.write().await;
        let member = state.members.get_mut(&(family_id, user_id))
            .ok_or(FamilyStorageError::MembershipNotFound { family_id, user_id })?;
        if member.role == FamilyRole::Owner { return Err(FamilyStorageError::OwnerRoleRequiresTransfer); }
        member.role = role;
        Ok(member.clone())
    }

    async fn change_member_relationship(&self, family_id: FamilyId, user_id: UserId, relationship: FamilyRelationship) -> Result<FamilyMember, FamilyStorageError> {
        let mut state = self.state.write().await;
        let member = state.members.get_mut(&(family_id, user_id))
            .ok_or(FamilyStorageError::MembershipNotFound { family_id, user_id })?;
        member.relationship = relationship;
        Ok(member.clone())
    }

    async fn transfer_ownership(&self, family_id: FamilyId, current_owner: UserId, new_owner: UserId) -> Result<(), FamilyStorageError> {
        let mut state = self.state.write().await;
        if !state.families.contains_key(&family_id) { return Err(FamilyStorageError::FamilyNotFound(family_id)); }
        let current_key = (family_id, current_owner);
        let new_key = (family_id, new_owner);
        let current = state.members.get(&current_key).ok_or(FamilyStorageError::MembershipNotFound { family_id, user_id: current_owner })?;
        if current.role != FamilyRole::Owner { return Err(FamilyStorageError::NotCurrentOwner); }
        if current_owner == new_owner { return Ok(()); }
        if !state.members.contains_key(&new_key) { return Err(FamilyStorageError::TransferTargetNotMember); }
        state.members.get_mut(&current_key).unwrap().role = FamilyRole::Admin;
        state.members.get_mut(&new_key).unwrap().role = FamilyRole::Owner;
        Ok(())
    }
}

impl InMemoryFamilyStore {
    async fn remove_non_owner(&self, family_id: FamilyId, user_id: UserId) -> Result<(), FamilyStorageError> {
        let mut state = self.state.write().await;
        if !state.families.contains_key(&family_id) { return Err(FamilyStorageError::FamilyNotFound(family_id)); }
        let key = (family_id, user_id);
        let member = state.members.get(&key).ok_or(FamilyStorageError::MembershipNotFound { family_id, user_id })?;
        if member.role == FamilyRole::Owner { return Err(FamilyStorageError::OwnerCannotLeave); }
        state.members.remove(&key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FamilyStore;
    use chrono::Utc;

    fn family(creator_id: UserId, name: &str) -> (Family, FamilyMember) {
        let id = FamilyId::new();
        let now = Utc::now();
        (Family { id, name: name.into(), creator_id, created_at: now, updated_at: now },
         FamilyMember { family_id: id, user_id: creator_id, role: FamilyRole::Owner, relationship: FamilyRelationship::Other, joined_at: now })
    }

    #[tokio::test]
    async fn family_creation_is_atomic_and_creates_one_owner() {
        let store = InMemoryFamilyStore::default();
        let owner = UserId::new();
        let (family, membership) = family(owner, "Home");
        store.create_family(family.clone(), membership).await.unwrap();
        let members = store.list_members(family.id).await.unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].role, FamilyRole::Owner);
    }

    #[tokio::test]
    async fn users_can_belong_to_multiple_families_and_duplicate_membership_is_rejected() {
        let store = InMemoryFamilyStore::default();
        let user = UserId::new();
        let (first, first_owner) = family(user, "First");
        let (second, second_owner) = family(user, "Second");
        store.create_family(first.clone(), first_owner).await.unwrap();
        store.create_family(second.clone(), second_owner).await.unwrap();
        assert_eq!(store.list_families(user).await.unwrap().len(), 2);
        let member = FamilyMember { family_id: first.id, user_id: user, role: FamilyRole::Member, relationship: FamilyRelationship::Partner, joined_at: Utc::now() };
        assert!(matches!(store.add_member(member).await, Err(FamilyStorageError::MembershipAlreadyExists { .. })));
    }

    #[tokio::test]
    async fn removing_membership_never_deletes_user_and_owner_invariant_is_preserved() {
        let store = InMemoryFamilyStore::default();
        let owner = UserId::new();
        let guest = UserId::new();
        let (family, owner_member) = family(owner, "Home");
        store.create_family(family.clone(), owner_member).await.unwrap();
        store.add_member(FamilyMember { family_id: family.id, user_id: guest, role: FamilyRole::Member, relationship: FamilyRelationship::Son, joined_at: Utc::now() }).await.unwrap();
        store.remove_member(family.id, guest).await.unwrap();
        assert_eq!(store.list_members(family.id).await.unwrap().len(), 1);
        assert_eq!(store.list_members(family.id).await.unwrap()[0].role, FamilyRole::Owner);
        assert!(matches!(store.leave_family(family.id, owner).await, Err(FamilyStorageError::OwnerCannotLeave)));
    }

    #[tokio::test]
    async fn ownership_transfer_is_atomic_and_preserves_one_owner() {
        let store = InMemoryFamilyStore::default();
        let owner = UserId::new();
        let next = UserId::new();
        let (family, owner_member) = family(owner, "Home");
        store.create_family(family.clone(), owner_member).await.unwrap();
        store.add_member(FamilyMember { family_id: family.id, user_id: next, role: FamilyRole::Admin, relationship: FamilyRelationship::Partner, joined_at: Utc::now() }).await.unwrap();
        store.transfer_ownership(family.id, owner, next).await.unwrap();
        let members = store.list_members(family.id).await.unwrap();
        assert_eq!(members.iter().filter(|m| m.role == FamilyRole::Owner).count(), 1);
        assert_eq!(members.iter().find(|m| m.user_id == owner).unwrap().role, FamilyRole::Admin);
        assert_eq!(members.iter().find(|m| m.user_id == next).unwrap().role, FamilyRole::Owner);
    }
}
