//! Tests for family authorization and membership workflows.

use super::*;
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
async fn trusted_context_contains_membership_details_and_rejects_non_members() {
    let (service, owner, _, family_id) = setup().await;
    let context = service.context_for_member(owner, family_id).await.unwrap();
    assert_eq!(context.family_id, family_id.to_string());
    assert_eq!(context.family_name, "Home");
    assert_eq!(context.family_role, "owner");
    assert_eq!(context.relationship, "other");

    assert!(matches!(
        service.context_for_member(UserId::new(), family_id).await,
        Err(FamilyServiceError::NotMember)
    ));
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
