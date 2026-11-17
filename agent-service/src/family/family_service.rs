//! Family management use-cases and centralized authorization policy.

use std::sync::Arc;

use agent_runtime::{FamilyContext, FamilyMemberContext};
use agent_storage::{
    Family, FamilyId, FamilyInvitationStore, FamilyInvitationSummary, FamilyMember,
    FamilyRelationship, FamilyRole, FamilyStore, InvitationTargetIdentity, InvitationToken, UserId,
};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::FamilyServiceError;

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
        let relationship = relationship_name(member.relationship);
        let members = self
            .members(family_id)
            .await?
            .into_iter()
            .map(|member| FamilyMemberContext {
                user_id: member.user_id.to_string(),
                name: None,
                role: role_name(member.role).to_owned(),
                relationship: relationship_name(member.relationship).to_owned(),
            })
            .collect();
        Ok(FamilyContext {
            family_id: family.id.to_string(),
            family_name: family.name,
            family_role: family_role.to_owned(),
            relationship: relationship.to_owned(),
            members,
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


fn role_name(role: FamilyRole) -> &'static str {
    match role {
        FamilyRole::Owner => "owner",
        FamilyRole::Admin => "admin",
        FamilyRole::Member => "member",
    }
}

fn relationship_name(relationship: FamilyRelationship) -> &'static str {
    match relationship {
        FamilyRelationship::Father => "father",
        FamilyRelationship::Mother => "mother",
        FamilyRelationship::Son => "son",
        FamilyRelationship::Daughter => "daughter",
        FamilyRelationship::Grandfather => "grandfather",
        FamilyRelationship::Grandmother => "grandmother",
        FamilyRelationship::Partner => "partner",
        FamilyRelationship::Other => "other",
    }
}
