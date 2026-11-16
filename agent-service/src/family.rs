use agent_runtime::FamilyContext;
//! Family workflows and authorization policy.
//!
//! HTTP and messaging transports should call this service rather than applying
//! family permissions themselves. The actor is always supplied by the trusted
//! authentication layer, never by a request body or an agent.
use std::sync::Arc;

use agent_storage::{
    Family, FamilyId, FamilyInvitationStore, FamilyInvitationSummary, FamilyMember,
    FamilyRelationship, FamilyRole, FamilyStorageError, FamilyStore, InvitationStorageError,
    InvitationTargetIdentity, InvitationToken, UserId,
};
use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

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

/// Family management use-cases and centralized role policy.
pub struct FamilyService {
    families: Arc<dyn FamilyStore>,
    invitations: Arc<dyn FamilyInvitationStore>,
}

impl FamilyService {
    pub fn new(
        families: Arc<dyn FamilyStore>,
        invitations: Arc<dyn FamilyInvitationStore>,
    ) -> Self {
        Self {
            families,
            invitations,
        }
    }

    async fn members(&self, family_id: FamilyId) -> Result<Vec<FamilyMember>, FamilyServiceError> {
        self.families
            .list_members(family_id)
            .await
            .map_err(Into::into)
    }

    async fn actor_member(
        &self,
        family_id: FamilyId,
        actor: UserId,
    ) -> Result<FamilyMember, FamilyServiceError> {
        self.members(family_id)
            .await?
            .into_iter()
            .find(|member| member.user_id == actor)
            .ok_or(FamilyServiceError::NotMember)
    }

    async fn require_admin(
        &self,
        family_id: FamilyId,
        actor: UserId,
    ) -> Result<FamilyMember, FamilyServiceError> {
        let member = self.actor_member(family_id, actor).await?;
        if !matches!(member.role, FamilyRole::Owner | FamilyRole::Admin) {
            return Err(FamilyServiceError::Forbidden);
        }
        Ok(member)
    }

    pub async fn create_family(
        &self,
        actor: UserId,
        name: &str,
        relationship: FamilyRelationship,
    ) -> Result<Family, FamilyServiceError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(FamilyServiceError::EmptyName);
        }
        let now = Utc::now();
        let family = Family {
            id: FamilyId::new(),
            name: name.to_owned(),
            creator_id: actor,
            created_at: now,
            updated_at: now,
        };
        let owner = FamilyMember {
            family_id: family.id,
            user_id: actor,
            role: FamilyRole::Owner,
            relationship,
            joined_at: now,
        };
        self.families.create_family(family.clone(), owner).await?;
        Ok(family)
    }

    pub async fn list_families(&self, actor: UserId) -> Result<Vec<Family>, FamilyServiceError> {
        self.families.list_families(actor).await.map_err(Into::into)
    }

    pub async fn get_family(
        &self,
        actor: UserId,
        family_id: FamilyId,
    ) -> Result<(Family, Vec<FamilyMember>), FamilyServiceError> {
        self.actor_member(family_id, actor).await?;
        let family = self
            .families
            .get_family(family_id)
            .await?
            .ok_or(FamilyServiceError::NotFound)?;
        let members = self.members(family_id).await?;
        Ok((family, members))
    }

    pub async fn rename_family(
        &self,
        actor: UserId,
        family_id: FamilyId,
        name: &str,
    ) -> Result<Family, FamilyServiceError> {
        self.require_admin(family_id, actor).await?;
        let name = name.trim();
        if name.is_empty() {
            return Err(FamilyServiceError::EmptyName);
        }
        self.families
            .rename_family(family_id, name)
            .await
            .map_err(Into::into)
    }

    pub async fn delete_family(
        &self,
        actor: UserId,
        family_id: FamilyId,
    ) -> Result<(), FamilyServiceError> {
        let member = self.actor_member(family_id, actor).await?;
        if member.role != FamilyRole::Owner {
            return Err(FamilyServiceError::Forbidden);
        }
        self.families
            .delete_family(family_id)
            .await
            .map_err(Into::into)
    }

    pub async fn list_members(
        &self,
        actor: UserId,
        family_id: FamilyId,
    ) -> Result<Vec<FamilyMember>, FamilyServiceError> {
        self.actor_member(family_id, actor).await?;
        self.members(family_id).await
    }

    /// Resolves a family context only when the actor is an active member.
    pub async fn context_for_member(
        &self,
        actor: UserId,
        family_id: FamilyId,
    ) -> Result<FamilyContext, FamilyServiceError> {
        let member = self.actor_member(family_id, actor).await?;
        let family = self
            .families
            .get_family(family_id)
            .await?
            .ok_or(FamilyServiceError::NotFound)?;
        let family_role = match member.role {
            FamilyRole::Owner => "owner",
            FamilyRole::Admin => "admin",
            FamilyRole::Member => "member",
        };
        let relationship = match member.relationship {
            FamilyRelationship::Father => "father",
            FamilyRelationship::Mother => "mother",
            FamilyRelationship::Son => "son",
            FamilyRelationship::Daughter => "daughter",
            FamilyRelationship::Grandfather => "grandfather",
            FamilyRelationship::Grandmother => "grandmother",
            FamilyRelationship::Partner => "partner",
            FamilyRelationship::Other => "other",
        };
        Ok(FamilyContext {
            family_id: family.id.to_string(),
            family_name: family.name,
            family_role: family_role.to_owned(),
            relationship: relationship.to_owned(),
        })
    }

    pub async fn create_invitation(
        &self,
        actor: UserId,
        family_id: FamilyId,
        target: Option<InvitationTargetIdentity>,
        expires_at: DateTime<Utc>,
    ) -> Result<(FamilyInvitationSummary, InvitationToken), FamilyServiceError> {
        self.require_admin(family_id, actor).await?;
        self.invitations
            .create_invitation(family_id, actor, target, expires_at)
            .await
            .map_err(Into::into)
    }

    pub async fn list_invitations(
        &self,
        actor: UserId,
        family_id: FamilyId,
    ) -> Result<Vec<FamilyInvitationSummary>, FamilyServiceError> {
        self.require_admin(family_id, actor).await?;
        self.invitations
            .list_invitations(family_id)
            .await
            .map_err(Into::into)
    }

    pub async fn revoke_invitation(
        &self,
        actor: UserId,
        family_id: FamilyId,
        invitation_id: Uuid,
    ) -> Result<FamilyInvitationSummary, FamilyServiceError> {
        self.require_admin(family_id, actor).await?;
        self.invitations
            .revoke_invitation(family_id, invitation_id)
            .await
            .map_err(Into::into)
    }

    /// A bearer invitation is the authorization for this operation. The
    /// storage adapter revalidates and consumes it atomically with membership.
    pub async fn accept_invitation(
        &self,
        token: &InvitationToken,
        identity: &InvitationTargetIdentity,
        user_id: UserId,
        relationship: FamilyRelationship,
    ) -> Result<FamilyMember, FamilyServiceError> {
        self.invitations
            .accept_invitation(token, identity, user_id, relationship)
            .await
            .map_err(Into::into)
    }

    pub async fn remove_member(
        &self,
        actor: UserId,
        family_id: FamilyId,
        target: UserId,
    ) -> Result<(), FamilyServiceError> {
        let actor_member = self.require_admin(family_id, actor).await?;
        let target_member =
            self.actor_member(family_id, target)
                .await
                .map_err(|error| match error {
                    FamilyServiceError::NotMember => FamilyServiceError::TargetNotMember,
                    other => other,
                })?;
        match actor_member.role {
            FamilyRole::Owner if target_member.role != FamilyRole::Owner => {}
            FamilyRole::Admin if target_member.role == FamilyRole::Member => {}
            _ => return Err(FamilyServiceError::Forbidden),
        }
        self.families
            .remove_member(family_id, target)
            .await
            .map_err(Into::into)
    }

    pub async fn leave_family(
        &self,
        actor: UserId,
        family_id: FamilyId,
    ) -> Result<(), FamilyServiceError> {
        self.actor_member(family_id, actor).await?;
        self.families
            .leave_family(family_id, actor)
            .await
            .map_err(Into::into)
    }

    pub async fn change_member_role(
        &self,
        actor: UserId,
        family_id: FamilyId,
        target: UserId,
        role: FamilyRole,
    ) -> Result<FamilyMember, FamilyServiceError> {
        if actor == target {
            return Err(FamilyServiceError::CannotChangeOwnRole);
        }
        if role == FamilyRole::Owner {
            return Err(FamilyServiceError::OwnerRoleRequiresTransfer);
        }
        let actor_member = self.require_admin(family_id, actor).await?;
        let target_member =
            self.actor_member(family_id, target)
                .await
                .map_err(|error| match error {
                    FamilyServiceError::NotMember => FamilyServiceError::TargetNotMember,
                    other => other,
                })?;
        match actor_member.role {
            FamilyRole::Owner if target_member.role != FamilyRole::Owner => {}
            // Admins may promote ordinary members, but cannot alter another
            // administrator's permissions or demote anyone.
            FamilyRole::Admin
                if target_member.role == FamilyRole::Member && role == FamilyRole::Admin => {}
            _ => return Err(FamilyServiceError::Forbidden),
        }
        self.families
            .change_member_role(family_id, target, role)
            .await
            .map_err(Into::into)
    }

    pub async fn change_member_relationship(
        &self,
        actor: UserId,
        family_id: FamilyId,
        target: UserId,
        relationship: FamilyRelationship,
    ) -> Result<FamilyMember, FamilyServiceError> {
        self.require_admin(family_id, actor).await?;
        self.actor_member(family_id, target)
            .await
            .map_err(|error| match error {
                FamilyServiceError::NotMember => FamilyServiceError::TargetNotMember,
                other => other,
            })?;
        self.families
            .change_member_relationship(family_id, target, relationship)
            .await
            .map_err(Into::into)
    }

    pub async fn transfer_ownership(
        &self,
        actor: UserId,
        family_id: FamilyId,
        new_owner: UserId,
    ) -> Result<(), FamilyServiceError> {
        let member = self.actor_member(family_id, actor).await?;
        if member.role != FamilyRole::Owner {
            return Err(FamilyServiceError::Forbidden);
        }
        self.families
            .transfer_ownership(family_id, actor, new_owner)
            .await
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_storage::{InMemoryFamilyInvitationStore, InMemoryFamilyStore};
    use chrono::Duration;

    async fn setup() -> (FamilyService, UserId, UserId, FamilyId) {
        let families = Arc::new(InMemoryFamilyStore::default());
        let invitations = Arc::new(InMemoryFamilyInvitationStore::new(families.clone()));
        let service = FamilyService::new(families, invitations);
        let owner = UserId::new();
        let family = service
            .create_family(owner, "  Home  ", FamilyRelationship::Other)
            .await
            .unwrap();
        let member = UserId::new();
        let (_, token) = service
            .create_invitation(owner, family.id, None, Utc::now() + Duration::days(1))
            .await
            .unwrap();
        service
            .accept_invitation(
                &token,
                &InvitationTargetIdentity::new("telegram", "123").unwrap(),
                member,
                FamilyRelationship::Son,
            )
            .await
            .unwrap();
        (service, owner, member, family.id)
    }

    #[tokio::test]
    async fn creates_family_with_owner_and_lists_it() {
        let (service, owner, _, family_id) = setup().await;
        let (family, members) = service.get_family(owner, family_id).await.unwrap();
        assert_eq!(family.name, "Home");
        assert_eq!(members.len(), 2);
        assert_eq!(
            members
                .iter()
                .filter(|m| m.role == FamilyRole::Owner)
                .count(),
            1
        );
        assert_eq!(service.list_families(owner).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn non_members_cannot_view_or_rename_family() {
        let (service, _, _, family_id) = setup().await;
        let outsider = UserId::new();
        assert!(matches!(
            service.get_family(outsider, family_id).await,
            Err(FamilyServiceError::NotMember)
        ));
        assert!(matches!(
            service.rename_family(outsider, family_id, "Nope").await,
            Err(FamilyServiceError::NotMember)
        ));
    }

    #[tokio::test]
    async fn members_cannot_invite_remove_or_change_roles() {
        let (service, _, member, family_id) = setup().await;
        assert!(matches!(
            service
                .create_invitation(member, family_id, None, Utc::now() + Duration::days(1))
                .await,
            Err(FamilyServiceError::Forbidden)
        ));
        assert!(matches!(
            service
                .change_member_role(member, family_id, member, FamilyRole::Admin)
                .await,
            Err(FamilyServiceError::CannotChangeOwnRole)
        ));
    }

    #[tokio::test]
    async fn admins_can_promote_members_but_cannot_change_other_admins() {
        let (service, owner, member, family_id) = setup().await;
        service
            .change_member_role(owner, family_id, member, FamilyRole::Admin)
            .await
            .unwrap();
        let second = UserId::new();
        let (_, token) = service
            .create_invitation(owner, family_id, None, Utc::now() + Duration::days(1))
            .await
            .unwrap();
        service
            .accept_invitation(
                &token,
                &InvitationTargetIdentity::new("telegram", "456").unwrap(),
                second,
                FamilyRelationship::Other,
            )
            .await
            .unwrap();
        assert!(matches!(
            service
                .change_member_role(member, family_id, second, FamilyRole::Member)
                .await,
            Err(FamilyServiceError::Forbidden)
        ));
    }

    #[tokio::test]
    async fn admins_cannot_remove_owner_or_other_admins() {
        let (service, owner, member, family_id) = setup().await;
        service
            .change_member_role(owner, family_id, member, FamilyRole::Admin)
            .await
            .unwrap();
        assert!(matches!(
            service.remove_member(member, family_id, owner).await,
            Err(FamilyServiceError::Forbidden)
        ));
        assert!(matches!(
            service.remove_member(member, family_id, member).await,
            Err(FamilyServiceError::Forbidden)
        ));
    }

    #[tokio::test]
    async fn owner_must_transfer_before_leaving_and_transfer_is_owner_only() {
        let (service, owner, member, family_id) = setup().await;
        assert!(matches!(
            service.leave_family(owner, family_id).await,
            Err(FamilyServiceError::OwnerMustTransfer)
        ));
        assert!(matches!(
            service.transfer_ownership(member, family_id, owner).await,
            Err(FamilyServiceError::Forbidden)
        ));
        service
            .transfer_ownership(owner, family_id, member)
            .await
            .unwrap();
        service.leave_family(owner, family_id).await.unwrap();
    }

    #[tokio::test]
    async fn cross_family_access_and_membership_mutations_are_rejected() {
        let (service, owner, _, family_id) = setup().await;

        let outsider = UserId::new();
        let other_family = service
            .create_family(outsider, "Other home", FamilyRelationship::Other)
            .await
            .unwrap();

        let other_member = UserId::new();
        let (_, token) = service
            .create_invitation(
                outsider,
                other_family.id,
                None,
                Utc::now() + Duration::days(1),
            )
            .await
            .unwrap();
        service
            .accept_invitation(
                &token,
                &InvitationTargetIdentity::new("telegram", "789").unwrap(),
                other_member,
                FamilyRelationship::Daughter,
            )
            .await
            .unwrap();

        // Family listing is scoped to the authenticated user.
        let owner_families = service.list_families(owner).await.unwrap();
        assert_eq!(owner_families.len(), 1);
        assert_eq!(owner_families[0].id, family_id);
        let outsider_families = service.list_families(outsider).await.unwrap();
        assert_eq!(outsider_families.len(), 1);
        assert_eq!(outsider_families[0].id, other_family.id);

        // A user from another family cannot inspect this family or its members.
        assert!(matches!(
            service.get_family(outsider, family_id).await,
            Err(FamilyServiceError::NotMember)
        ));
        assert!(matches!(
            service.list_members(outsider, family_id).await,
            Err(FamilyServiceError::NotMember)
        ));
        assert!(matches!(
            service.list_invitations(outsider, family_id).await,
            Err(FamilyServiceError::NotMember)
        ));

        // Even an owner cannot mutate a target through a different family id.
        assert!(matches!(
            service.remove_member(owner, family_id, other_member).await,
            Err(FamilyServiceError::TargetNotMember)
        ));
        assert!(matches!(
            service
                .change_member_role(owner, family_id, other_member, FamilyRole::Admin)
                .await,
            Err(FamilyServiceError::TargetNotMember)
        ));
        assert!(matches!(
            service
                .change_member_relationship(
                    owner,
                    family_id,
                    other_member,
                    FamilyRelationship::Son,
                )
                .await,
            Err(FamilyServiceError::TargetNotMember)
        ));

        // Rejected cross-family operations leave the original membership intact.
        let (_, members) = service.get_family(outsider, other_family.id).await.unwrap();
        assert!(members.iter().any(|member| {
            member.user_id == other_member
                && member.role == FamilyRole::Member
                && member.relationship == FamilyRelationship::Daughter
        }));
    }

    #[tokio::test]
    async fn invitation_creation_and_revocation_require_admin_role() {
        let (service, owner, member, family_id) = setup().await;
        let (invitation, _) = service
            .create_invitation(owner, family_id, None, Utc::now() + Duration::days(1))
            .await
            .unwrap();
        assert_eq!(
            service
                .list_invitations(owner, family_id)
                .await
                .unwrap()
                .len(),
            2
        );
        service
            .revoke_invitation(owner, family_id, invitation.id)
            .await
            .unwrap();
        assert!(matches!(
            service
                .revoke_invitation(member, family_id, invitation.id)
                .await,
            Err(FamilyServiceError::Forbidden)
        ));
    }
}
