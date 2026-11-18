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
    assert_eq!(
        reopened
            .find_identity_for_user("telegram", second_user.id)
            .unwrap()
            .unwrap(),
        second_identity
    );
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

#[test]
fn display_name_can_be_updated_and_survives_reopening_store() {
    let path = temporary_file();
    let user = {
        let store = FileUserIdentityStore::open(&path).unwrap();
        store
            .resolve_or_create("telegram", "987", Some("Telegram"))
            .unwrap()
            .0
    };
    let updated = FileUserIdentityStore::open(&path)
        .unwrap()
        .update_display_name(user.id, "  Javier Sogo  ")
        .unwrap();
    assert_eq!(updated.display_name.as_deref(), Some("Javier Sogo"));
    let reopened = FileUserIdentityStore::open(&path).unwrap();
    assert_eq!(
        reopened
            .get_user(user.id)
            .unwrap()
            .unwrap()
            .display_name
            .as_deref(),
        Some("Javier Sogo")
    );
    fs::remove_file(path).unwrap();
}

#[test]
fn display_name_update_rejects_empty_or_too_long_names() {
    let store = InMemoryUserIdentityStore::default();
    let (user, _) = store.resolve_or_create("telegram", "988", None).unwrap();
    assert!(matches!(
        store.update_display_name(user.id, "  "),
        Err(IdentityStoreError::InvalidDisplayName)
    ));
    assert!(matches!(
        store.update_display_name(user.id, &"x".repeat(81)),
        Err(IdentityStoreError::InvalidDisplayName)
    ));
    assert!(matches!(
        store.update_display_name(UserId::new(), "Javier"),
        Err(IdentityStoreError::UserNotFound(_))
    ));
}

#[test]
fn external_identity_can_be_resolved_from_its_eme_user() {
    let store = InMemoryUserIdentityStore::default();
    let (user, telegram_identity) = store
        .resolve_or_create("telegram", "123456789", Some("Javier"))
        .unwrap();

    let resolved = store
        .find_identity_for_user("telegram", user.id)
        .unwrap()
        .unwrap();

    assert_eq!(resolved, telegram_identity);
    assert_eq!(resolved.subject, "123456789");
    assert!(
        store
            .find_identity_for_user("google", user.id)
            .unwrap()
            .is_none()
    );
}

#[test]
fn managed_user_has_no_telegram_identity() {
    let store = InMemoryUserIdentityStore::default();
    let user = store.create_user("Alex", None).unwrap();

    assert!(
        store
            .find_identity_for_user("telegram", user.id)
            .unwrap()
            .is_none()
    );
}

#[test]
fn managed_user_has_no_external_identity_and_persists_birthday() {
    use chrono::NaiveDate;
    let path = temporary_file();
    let user = FileUserIdentityStore::open(&path)
        .unwrap()
        .create_user("Alex", Some(NaiveDate::from_ymd_opt(2018, 5, 12).unwrap()))
        .unwrap();
    let reopened = FileUserIdentityStore::open(&path).unwrap();
    assert_eq!(
        reopened.get_user(user.id).unwrap().unwrap().birthday,
        user.birthday
    );
    assert!(reopened.find_identity("telegram", "123").unwrap().is_none());
    fs::remove_file(path).unwrap();
}

#[test]
fn timezone_can_be_updated_and_survives_reopening_store() {
    let path = temporary_file();
    let user = FileUserIdentityStore::open(&path)
        .unwrap()
        .resolve_or_create("telegram", "timezone-user", None)
        .unwrap()
        .0;

    let updated = FileUserIdentityStore::open(&path)
        .unwrap()
        .update_timezone(user.id, Some("Europe/Madrid"))
        .unwrap();
    assert_eq!(updated.timezone.as_deref(), Some("Europe/Madrid"));

    let reopened = FileUserIdentityStore::open(&path).unwrap();
    assert_eq!(
        reopened.get_user(user.id).unwrap().unwrap().timezone.as_deref(),
        Some("Europe/Madrid")
    );
    fs::remove_file(path).unwrap();
}

#[test]
fn timezone_rejects_non_iana_identifiers() {
    let store = InMemoryUserIdentityStore::default();
    let user = store.resolve_or_create("telegram", "invalid-timezone-user", None).unwrap().0;
    assert!(matches!(
        store.update_timezone(user.id, Some("Madrid")),
        Err(IdentityStoreError::InvalidTimezone)
    ));
}
