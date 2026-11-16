//! Tests for external user identity resolution and linking.

use std::{fs, path::PathBuf};
use uuid::Uuid;

use super::*;

fn temporary_file() -> PathBuf {
    std::env::temp_dir().join(format!("eme-identities-{}.json", Uuid::new_v4()))
}

#[test]
fn telegram_identity_resolves_to_the_same_user_after_reopening_store() {
    let path = temporary_file();
    let (first_user, first_identity) = {
        let store = FileUserIdentityStore::open(&path).unwrap();
        store
            .resolve_or_create("telegram", "123456", Some("Javier"))
            .unwrap()
    };
    let reopened = FileUserIdentityStore::open(&path).unwrap();
    let (second_user, second_identity) = reopened
        .resolve_or_create("telegram", "123456", Some("Changed name"))
        .unwrap();
    assert_eq!(first_user.id, second_user.id);
    assert_eq!(first_identity, second_identity);
    assert_eq!(second_user.display_name.as_deref(), Some("Javier"));
    fs::remove_file(path).unwrap();
}

#[test]
fn one_external_identity_cannot_be_linked_to_two_users() {
    let store = InMemoryUserIdentityStore::default();
    let (first, _) = store.resolve_or_create("telegram", "42", None).unwrap();
    let (second, _) = store
        .resolve_or_create("google", "person@example.com", None)
        .unwrap();
    assert!(matches!(
        store.link_identity(second.id, "telegram", "42"),
        Err(IdentityStoreError::IdentityAlreadyLinked { .. })
    ));
    assert_eq!(
        store
            .find_identity("telegram", "42")
            .unwrap()
            .unwrap()
            .user_id,
        first.id
    );
}

#[test]
fn a_user_can_have_multiple_external_identities() {
    let store = InMemoryUserIdentityStore::default();
    let (user, _) = store.resolve_or_create("telegram", "42", None).unwrap();
    store
        .link_identity(user.id, "google", "person@example.com")
        .unwrap();
    assert_eq!(
        store
            .find_identity("google", "person@example.com")
            .unwrap()
            .unwrap()
            .user_id,
        user.id
    );
}
