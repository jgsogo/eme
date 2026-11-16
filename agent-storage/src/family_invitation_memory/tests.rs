
use crate::*;
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
