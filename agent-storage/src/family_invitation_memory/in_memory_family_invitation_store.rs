//! Process-local invitation store with serialized acceptance.

use super::State;
use crate::{
    FamilyId, FamilyInvitation, FamilyInvitationStore, FamilyInvitationSummary, FamilyMember,
    FamilyRelationship, FamilyRole, FamilyStore, InvitationStorageError, InvitationTargetIdentity,
    InvitationToken, UserId,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

pub struct InMemoryFamilyInvitationStore {
    families: Arc<dyn FamilyStore>,
    state: Mutex<State>,
}

impl InMemoryFamilyInvitationStore {
    pub fn new(families: Arc<dyn FamilyStore>) -> Self {
        Self {
            families,
            state: Mutex::new(State::default()),
        }
    }
}

#[async_trait]
impl FamilyInvitationStore for InMemoryFamilyInvitationStore {
    async fn create_invitation(
        &self,
        family_id: FamilyId,
        inviter_id: UserId,
        target_identity: Option<InvitationTargetIdentity>,
        expires_at: DateTime<Utc>,
    ) -> Result<(FamilyInvitationSummary, InvitationToken), InvitationStorageError> {
        let now = Utc::now();
        if expires_at <= now {
            return Err(InvitationStorageError::InvalidExpiry);
        }
        let members = self
            .families
            .list_members(family_id)
            .await
            .map_err(|_| InvitationStorageError::FamilyNotFound(family_id))?;
        if !members.iter().any(|m| m.user_id == inviter_id) {
            return Err(InvitationStorageError::InviterNotMember);
        }
        let token = InvitationToken::generate();
        let invitation = FamilyInvitation {
            id: Uuid::new_v4(),
            family_id,
            inviter_id,
            target_identity,
            token_hash: token.hash(),
            created_at: now,
            expires_at,
            accepted_at: None,
            accepted_by: None,
            revoked_at: None,
        };
        let summary = FamilyInvitationSummary::from(&invitation);
        let mut state = self.state.lock().await;
        state
            .token_index
            .insert(invitation.token_hash.clone(), invitation.id);
        state.invitations.insert(invitation.id, invitation);
        Ok((summary, token))
    }

    async fn list_invitations(
        &self,
        family_id: FamilyId,
    ) -> Result<Vec<FamilyInvitationSummary>, InvitationStorageError> {
        self.families
            .get_family(family_id)
            .await
            .map_err(|_| InvitationStorageError::FamilyNotFound(family_id))?
            .ok_or(InvitationStorageError::FamilyNotFound(family_id))?;
        let state = self.state.lock().await;
        let mut result: Vec<_> = state
            .invitations
            .values()
            .filter(|i| i.family_id == family_id)
            .map(FamilyInvitationSummary::from)
            .collect();
        result.sort_by_key(|i| i.created_at);
        Ok(result)
    }

    async fn revoke_invitation(
        &self,
        family_id: FamilyId,
        invitation_id: Uuid,
    ) -> Result<FamilyInvitationSummary, InvitationStorageError> {
        let mut state = self.state.lock().await;
        let invitation = state
            .invitations
            .get_mut(&invitation_id)
            .filter(|i| i.family_id == family_id)
            .ok_or(InvitationStorageError::NotFound)?;
        if invitation.accepted_at.is_some() {
            return Err(InvitationStorageError::AlreadyAccepted);
        }
        if invitation.revoked_at.is_none() {
            invitation.revoked_at = Some(Utc::now());
        }
        Ok(FamilyInvitationSummary::from(&*invitation))
    }

    async fn validate_invitation(
        &self,
        token: &InvitationToken,
        accepting_identity: &InvitationTargetIdentity,
    ) -> Result<(), InvitationStorageError> {
        let state = self.state.lock().await;
        let id = *state
            .token_index
            .get(&token.hash())
            .ok_or(InvitationStorageError::NotFound)?;
        let invitation = state
            .invitations
            .get(&id)
            .ok_or(InvitationStorageError::NotFound)?;
        if invitation
            .target_identity
            .as_ref()
            .is_some_and(|target| target != accepting_identity)
        {
            return Err(InvitationStorageError::IdentityMismatch);
        }
        if invitation.accepted_at.is_some() {
            return Err(InvitationStorageError::AlreadyAccepted);
        }
        if invitation.revoked_at.is_some() {
            return Err(InvitationStorageError::Revoked);
        }
        if invitation.expires_at <= Utc::now() {
            return Err(InvitationStorageError::Expired);
        }
        Ok(())
    }

    async fn accept_invitation(
        &self,
        token: &InvitationToken,
        accepting_identity: &InvitationTargetIdentity,
        user_id: UserId,
        relationship: FamilyRelationship,
    ) -> Result<FamilyMember, InvitationStorageError> {
        let token_hash = token.hash();
        // Serialize acceptance through membership creation to make retries safe.
        let mut state = self.state.lock().await;
        let id = *state
            .token_index
            .get(&token_hash)
            .ok_or(InvitationStorageError::NotFound)?;
        let invitation = state
            .invitations
            .get(&id)
            .ok_or(InvitationStorageError::NotFound)?;
        if invitation
            .target_identity
            .as_ref()
            .is_some_and(|target| target != accepting_identity)
        {
            return Err(InvitationStorageError::IdentityMismatch);
        }
        if let Some(accepted_by) = invitation.accepted_by {
            if accepted_by != user_id {
                return Err(InvitationStorageError::AlreadyAccepted);
            }
            let family_id = invitation.family_id;
            let members = self
                .families
                .list_members(family_id)
                .await
                .map_err(|_| InvitationStorageError::FamilyNotFound(family_id))?;
            return members
                .into_iter()
                .find(|m| m.user_id == user_id)
                .ok_or(InvitationStorageError::AlreadyAccepted);
        }
        if invitation.revoked_at.is_some() {
            return Err(InvitationStorageError::Revoked);
        }
        if invitation.expires_at <= Utc::now() {
            return Err(InvitationStorageError::Expired);
        }

        let family_id = invitation.family_id;
        let member = FamilyMember {
            family_id,
            user_id,
            role: FamilyRole::Member,
            relationship,
            joined_at: Utc::now(),
        };
        self.families
            .add_member(member.clone())
            .await
            .map_err(|error| match error {
                crate::FamilyStorageError::MembershipAlreadyExists { .. } => {
                    InvitationStorageError::MembershipAlreadyExists
                }
                crate::FamilyStorageError::FamilyNotFound(_) => {
                    InvitationStorageError::FamilyNotFound(family_id)
                }
                _ => InvitationStorageError::MembershipCreationFailed,
            })?;
        let invitation = state
            .invitations
            .get_mut(&id)
            .expect("locked invitation exists");
        invitation.accepted_at = Some(Utc::now());
        invitation.accepted_by = Some(user_id);
        Ok(member)
    }
}
