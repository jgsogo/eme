//! In-memory invitation storage. Acceptance is serialized per store instance.
use crate::{
    FamilyId, FamilyInvitation, FamilyInvitationStore, FamilyInvitationSummary, FamilyMember,
    FamilyRelationship, FamilyRole, FamilyStore, InvitationStorageError, InvitationTargetIdentity,
    InvitationToken, UserId,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Default)]
struct State {
    invitations: HashMap<Uuid, FamilyInvitation>,
    token_index: HashMap<String, Uuid>,
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Family, InMemoryFamilyStore};
    use chrono::Duration;

    async fn setup() -> (
        Arc<InMemoryFamilyStore>,
        InMemoryFamilyInvitationStore,
        FamilyId,
        UserId,
    ) {
        let families = Arc::new(InMemoryFamilyStore::default());
        let owner = UserId::new();
        let family_id = FamilyId::new();
        let now = Utc::now();
        families
            .create_family(
                Family {
                    id: family_id,
                    name: "Home".into(),
                    creator_id: owner,
                    created_at: now,
                    updated_at: now,
                },
                FamilyMember {
                    family_id,
                    user_id: owner,
                    role: FamilyRole::Owner,
                    relationship: FamilyRelationship::Other,
                    joined_at: now,
                },
            )
            .await
            .unwrap();
        (
            families.clone(),
            InMemoryFamilyInvitationStore::new(families),
            family_id,
            owner,
        )
    }

    fn identity(subject: &str) -> InvitationTargetIdentity {
        InvitationTargetIdentity::new("telegram", subject).unwrap()
    }

    #[tokio::test]
    async fn token_is_only_returned_at_creation_and_listing_has_no_secret() {
        let (_, store, family, owner) = setup().await;
        let (created, token) = store
            .create_invitation(family, owner, None, Utc::now() + Duration::days(1))
            .await
            .unwrap();
        let listed = store.list_invitations(family).await.unwrap();
        assert_eq!(listed, vec![created]);
        let json = serde_json::to_string(&listed).unwrap();
        assert!(!json.contains(token.expose()));
        assert!(!format!("{:?}", token).contains(token.expose()));
        assert_eq!(token.expose().len(), 64);
    }

    #[tokio::test]
    async fn rejects_invalid_expired_revoked_and_identity_mismatched_tokens() {
        let (_, store, family, owner) = setup().await;
        assert!(matches!(
            store
                .create_invitation(family, owner, None, Utc::now() - Duration::seconds(1))
                .await,
            Err(InvitationStorageError::InvalidExpiry)
        ));
        let (_, bound) = store
            .create_invitation(
                family,
                owner,
                Some(identity("42")),
                Utc::now() + Duration::days(1),
            )
            .await
            .unwrap();
        assert!(matches!(
            store
                .accept_invitation(
                    &bound,
                    &identity("99"),
                    UserId::new(),
                    FamilyRelationship::Other
                )
                .await,
            Err(InvitationStorageError::IdentityMismatch)
        ));
        let (summary, revoked) = store
            .create_invitation(family, owner, None, Utc::now() + Duration::days(1))
            .await
            .unwrap();
        store.revoke_invitation(family, summary.id).await.unwrap();
        assert!(matches!(
            store
                .accept_invitation(
                    &revoked,
                    &identity("1"),
                    UserId::new(),
                    FamilyRelationship::Other
                )
                .await,
            Err(InvitationStorageError::Revoked)
        ));
        let (_, expired) = store
            .create_invitation(family, owner, None, Utc::now() + Duration::milliseconds(1))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        assert!(matches!(
            store
                .accept_invitation(
                    &expired,
                    &identity("1"),
                    UserId::new(),
                    FamilyRelationship::Other
                )
                .await,
            Err(InvitationStorageError::Expired)
        ));
        assert!(matches!(
            store
                .accept_invitation(
                    &InvitationToken::generate(),
                    &identity("1"),
                    UserId::new(),
                    FamilyRelationship::Other
                )
                .await,
            Err(InvitationStorageError::NotFound)
        ));
    }

    #[tokio::test]
    async fn acceptance_is_single_use_and_idempotent_for_same_user() {
        let (families, store, family, owner) = setup().await;
        let (_, token) = store
            .create_invitation(
                family,
                owner,
                Some(identity("123")),
                Utc::now() + Duration::days(1),
            )
            .await
            .unwrap();
        let user = UserId::new();
        let first = store
            .accept_invitation(&token, &identity("123"), user, FamilyRelationship::Son)
            .await
            .unwrap();
        let retry = store
            .accept_invitation(&token, &identity("123"), user, FamilyRelationship::Other)
            .await
            .unwrap();
        assert_eq!(first, retry);
        assert!(matches!(
            store
                .accept_invitation(
                    &token,
                    &identity("123"),
                    UserId::new(),
                    FamilyRelationship::Son
                )
                .await,
            Err(InvitationStorageError::AlreadyAccepted)
        ));
        assert_eq!(
            families
                .list_members(family)
                .await
                .unwrap()
                .iter()
                .filter(|m| m.user_id == user)
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn concurrent_acceptance_creates_only_one_membership() {
        let (families, store, family, owner) = setup().await;
        let (_, token) = store
            .create_invitation(family, owner, None, Utc::now() + Duration::days(1))
            .await
            .unwrap();
        let store = Arc::new(store);
        let user = UserId::new();
        let mut tasks = Vec::new();
        for _ in 0..2 {
            let store = store.clone();
            let token = token.clone();
            tasks.push(tokio::spawn(async move {
                store
                    .accept_invitation(&token, &identity("1"), user, FamilyRelationship::Son)
                    .await
            }));
        }
        for task in tasks {
            task.await.unwrap().unwrap();
        }
        assert_eq!(
            families
                .list_members(family)
                .await
                .unwrap()
                .iter()
                .filter(|m| m.user_id == user)
                .count(),
            1
        );
    }
}
