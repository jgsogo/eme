    use crate::*;
    use crate::FamilyStore;
    use chrono::Utc;

    fn family(creator_id: UserId, name: &str) -> (Family, FamilyMember) {
        let id = FamilyId::new();
        let now = Utc::now();
        (
            Family {
                id,
                name: name.into(),
                creator_id,
                created_at: now,
                updated_at: now,
            },
            FamilyMember {
                family_id: id,
                user_id: creator_id,
                role: FamilyRole::Owner,
                relationship: FamilyRelationship::Other,
                joined_at: now,
            },
        )
    }

    #[tokio::test]
    async fn family_creation_is_atomic_and_creates_one_owner() {
        let store = InMemoryFamilyStore::default();
        let owner = UserId::new();
        let (family, membership) = family(owner, "Home");
        store
            .create_family(family.clone(), membership)
            .await
            .unwrap();
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
        store
            .create_family(first.clone(), first_owner)
            .await
            .unwrap();
        store
            .create_family(second.clone(), second_owner)
            .await
            .unwrap();
        assert_eq!(store.list_families(user).await.unwrap().len(), 2);
        let member = FamilyMember {
            family_id: first.id,
            user_id: user,
            role: FamilyRole::Member,
            relationship: FamilyRelationship::Partner,
            joined_at: Utc::now(),
        };
        assert!(matches!(
            store.add_member(member).await,
            Err(FamilyStorageError::MembershipAlreadyExists { .. })
        ));
    }

    #[tokio::test]
    async fn removing_membership_never_deletes_user_and_owner_invariant_is_preserved() {
        let store = InMemoryFamilyStore::default();
        let owner = UserId::new();
        let guest = UserId::new();
        let (family, owner_member) = family(owner, "Home");
        store
            .create_family(family.clone(), owner_member)
            .await
            .unwrap();
        store
            .add_member(FamilyMember {
                family_id: family.id,
                user_id: guest,
                role: FamilyRole::Member,
                relationship: FamilyRelationship::Son,
                joined_at: Utc::now(),
            })
            .await
            .unwrap();
        store.remove_member(family.id, guest).await.unwrap();
        assert_eq!(store.list_members(family.id).await.unwrap().len(), 1);
        assert_eq!(
            store.list_members(family.id).await.unwrap()[0].role,
            FamilyRole::Owner
        );
        assert!(matches!(
            store.leave_family(family.id, owner).await,
            Err(FamilyStorageError::OwnerCannotLeave)
        ));
    }

    #[tokio::test]
    async fn ownership_transfer_is_atomic_and_preserves_one_owner() {
        let store = InMemoryFamilyStore::default();
        let owner = UserId::new();
        let next = UserId::new();
        let (family, owner_member) = family(owner, "Home");
        store
            .create_family(family.clone(), owner_member)
            .await
            .unwrap();
        store
            .add_member(FamilyMember {
                family_id: family.id,
                user_id: next,
                role: FamilyRole::Admin,
                relationship: FamilyRelationship::Partner,
                joined_at: Utc::now(),
            })
            .await
            .unwrap();
        store
            .transfer_ownership(family.id, owner, next)
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
        assert_eq!(
            members.iter().find(|m| m.user_id == owner).unwrap().role,
            FamilyRole::Admin
        );
        assert_eq!(
            members.iter().find(|m| m.user_id == next).unwrap().role,
            FamilyRole::Owner
        );
    }
