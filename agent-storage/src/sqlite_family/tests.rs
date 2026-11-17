//! Tests for SQLite family, membership, and invitation persistence.

use chrono::Duration;

use super::*;

fn database_path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("eme-family-{}.sqlite", Uuid::new_v4()))
}

fn family(owner: UserId) -> (Family, FamilyMember) {
    let id = FamilyId::new();
    let now = Utc::now();
    (
        Family {
            id,
            name: "Home".into(),
            creator_id: owner,
            created_at: now,
            updated_at: now,
        },
        FamilyMember {
            family_id: id,
            user_id: owner,
            role: FamilyRole::Owner,
            relationship: FamilyRelationship::Other,
            joined_at: now,
        },
    )
}

fn identity(subject: &str) -> InvitationTargetIdentity {
    InvitationTargetIdentity::new("telegram", subject).unwrap()
}

#[tokio::test]
async fn persists_families_members_and_invitations_across_reopen() {
    let path = database_path();
    let owner = UserId::new();
    let (family, membership) = family(owner);
    let invitation_id;
    {
        let store = SqliteFamilyStore::open(&path).unwrap();
        store
            .create_family(family.clone(), membership)
            .await
            .unwrap();
        let (invitation, _) = store
            .create_invitation(
                family.id,
                owner,
                Some(identity("telegram-user")),
                Utc::now() + Duration::days(1),
            )
            .await
            .unwrap();
        invitation_id = invitation.id;
    }
    {
        let store = SqliteFamilyStore::open(&path).unwrap();
        assert_eq!(
            store.get_family(family.id).await.unwrap(),
            Some(family.clone())
        );
        assert_eq!(store.list_members(family.id).await.unwrap().len(), 1);
        let invitations = store.list_invitations(family.id).await.unwrap();
        assert_eq!(invitations.len(), 1);
        assert_eq!(invitations[0].id, invitation_id);
        assert_eq!(
            invitations[0].target_identity,
            Some(identity("telegram-user"))
        );
    }
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn acceptance_is_transactional_single_use_and_idempotent() {
    let path = database_path();
    let store = SqliteFamilyStore::open(&path).unwrap();
    let owner = UserId::new();
    let (family, membership) = family(owner);
    store
        .create_family(family.clone(), membership)
        .await
        .unwrap();
    let (_, token) = store
        .create_invitation(
            family.id,
            owner,
            Some(identity("telegram-user")),
            Utc::now() + Duration::days(1),
        )
        .await
        .unwrap();
    let guest = UserId::new();
    let accepted = store
        .accept_invitation(
            &token,
            &identity("telegram-user"),
            guest,
            FamilyRelationship::Son,
        )
        .await
        .unwrap();
    assert_eq!(accepted.user_id, guest);
    let retry = store
        .accept_invitation(
            &token,
            &identity("telegram-user"),
            guest,
            FamilyRelationship::Other,
        )
        .await
        .unwrap();
    assert_eq!(retry, accepted);
    assert!(matches!(
        store
            .accept_invitation(
                &token,
                &identity("telegram-user"),
                UserId::new(),
                FamilyRelationship::Son
            )
            .await,
        Err(InvitationStorageError::AlreadyAccepted)
    ));
    assert_eq!(store.list_members(family.id).await.unwrap().len(), 2);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn ownership_transfer_and_member_removal_preserve_invariants() {
    let path = database_path();
    let store = SqliteFamilyStore::open(&path).unwrap();
    let owner = UserId::new();
    let guest = UserId::new();
    let (family, membership) = family(owner);
    store
        .create_family(family.clone(), membership)
        .await
        .unwrap();
    store
        .add_member(FamilyMember {
            family_id: family.id,
            user_id: guest,
            role: FamilyRole::Admin,
            relationship: FamilyRelationship::Partner,
            joined_at: Utc::now(),
        })
        .await
        .unwrap();
    store
        .transfer_ownership(family.id, owner, guest)
        .await
        .unwrap();
    let members = store.list_members(family.id).await.unwrap();
    assert_eq!(
        members
            .iter()
            .filter(|m| m.role == FamilyRole::Owner)
            .count(),
        1
    );
    store.remove_member(family.id, owner).await.unwrap();
    assert_eq!(store.list_members(family.id).await.unwrap().len(), 1);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn rejects_expired_revoked_and_identity_mismatched_invitations() {
    let path = database_path();
    let store = SqliteFamilyStore::open(&path).unwrap();
    let owner = UserId::new();
    let (family, membership) = family(owner);
    store
        .create_family(family.clone(), membership)
        .await
        .unwrap();
    let (wrong_identity, token) = store
        .create_invitation(
            family.id,
            owner,
            Some(identity("allowed")),
            Utc::now() + Duration::days(1),
        )
        .await
        .unwrap();
    assert!(matches!(
        store
            .accept_invitation(
                &token,
                &identity("other"),
                UserId::new(),
                FamilyRelationship::Other
            )
            .await,
        Err(InvitationStorageError::IdentityMismatch)
    ));
    store
        .revoke_invitation(family.id, wrong_identity.id)
        .await
        .unwrap();
    assert!(matches!(
        store
            .accept_invitation(
                &token,
                &identity("allowed"),
                UserId::new(),
                FamilyRelationship::Other
            )
            .await,
        Err(InvitationStorageError::Revoked)
    ));
    let (_, expired) = store
        .create_invitation(family.id, owner, None, Utc::now() + Duration::days(1))
        .await
        .unwrap();
    {
        let connection = store.connection.lock().await;
        connection
            .execute(
                "UPDATE family_invitations SET created_at = ?1, expires_at = ?2 WHERE token_hash = ?3",
                params![
                    (Utc::now() - Duration::seconds(2)).to_rfc3339(),
                    (Utc::now() - Duration::seconds(1)).to_rfc3339(),
                    expired.hash()
                ],
            )
            .unwrap();
    }
    assert!(matches!(
        store
            .accept_invitation(
                &expired,
                &identity("telegram"),
                UserId::new(),
                FamilyRelationship::Other
            )
            .await,
        Err(InvitationStorageError::Expired)
    ));
    let _ = std::fs::remove_file(path);
}
