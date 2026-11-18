//! Tests for file-backed conversation persistence.

use std::{env, fs};
use uuid::Uuid;

use agent_runtime::{Message, ToolCall};
use serde_json::json;

use super::{ConversationStore, ConversationStoreError};
use crate::FamilyId;

fn temporary_directory() -> std::path::PathBuf {
    env::temp_dir().join(format!("agent-storage-test-{}", Uuid::new_v4()))
}

#[test]
fn creates_active_conversation_and_persists_it_across_reopen() {
    let directory = temporary_directory();
    let store = ConversationStore::open(&directory).unwrap();

    let conversation = store.create("telegram:123:456", "assistant").unwrap();
    assert_eq!(
        store.active("telegram:123:456").unwrap().unwrap().id,
        conversation.id
    );

    drop(store);
    let reopened = ConversationStore::open(&directory).unwrap();
    let loaded = reopened.active("telegram:123:456").unwrap().unwrap();

    assert_eq!(loaded.id, conversation.id);
    assert_eq!(loaded.agent, "assistant");
    assert_eq!(loaded.title, "New conversation");
    assert!(loaded.messages.is_empty());

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn selected_family_is_persisted_and_can_be_cleared() {
    let directory = temporary_directory();
    let store = ConversationStore::open(&directory).unwrap();
    let conversation = store.create("owner-a", "assistant").unwrap();
    let family_id = FamilyId::new();

    let selected = store
        .set_selected_family("owner-a", conversation.id, Some(family_id))
        .unwrap();
    assert_eq!(selected.selected_family_id, Some(family_id));

    drop(store);
    let reopened = ConversationStore::open(&directory).unwrap();
    assert_eq!(
        reopened.get(conversation.id).unwrap().selected_family_id,
        Some(family_id)
    );
    reopened
        .set_selected_family("owner-a", conversation.id, None)
        .unwrap();
    assert_eq!(
        reopened.get(conversation.id).unwrap().selected_family_id,
        None
    );

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cannot_change_selected_family_of_another_owners_conversation() {
    let directory = temporary_directory();
    let store = ConversationStore::open(&directory).unwrap();
    let conversation = store.create("owner-a", "assistant").unwrap();

    assert!(matches!(
        store.set_selected_family("owner-b", conversation.id, Some(FamilyId::new())),
        Err(ConversationStoreError::OwnerMismatch { .. })
    ));
    assert_eq!(store.get(conversation.id).unwrap().selected_family_id, None);

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn renames_conversation_and_persists_the_title() {
    let directory = temporary_directory();
    let store = ConversationStore::open(&directory).unwrap();
    let conversation = store.create("owner-a", "assistant").unwrap();

    let renamed = store
        .rename("owner-a", conversation.id, "  Project planning  ")
        .unwrap();
    assert_eq!(renamed.title, "Project planning");

    drop(store);
    let reopened = ConversationStore::open(&directory).unwrap();
    assert_eq!(
        reopened.get(conversation.id).unwrap().title,
        "Project planning"
    );

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cannot_rename_another_owners_conversation_or_use_an_empty_title() {
    let directory = temporary_directory();
    let store = ConversationStore::open(&directory).unwrap();
    let conversation = store.create("owner-a", "assistant").unwrap();

    assert!(matches!(
        store.rename("owner-b", conversation.id, "Private title"),
        Err(ConversationStoreError::OwnerMismatch { .. })
    ));
    assert!(matches!(
        store.rename("owner-a", conversation.id, "  "),
        Err(ConversationStoreError::EmptyTitle)
    ));

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn deletes_conversation_and_clears_its_active_pointer() {
    let directory = temporary_directory();
    let store = ConversationStore::open(&directory).unwrap();
    let conversation = store.create("owner-a", "assistant").unwrap();

    store.delete("owner-a", conversation.id).unwrap();

    assert!(matches!(
        store.get(conversation.id),
        Err(ConversationStoreError::NotFound(_))
    ));
    assert!(store.active("owner-a").unwrap().is_none());
    assert!(store.list("owner-a").unwrap().is_empty());

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cannot_delete_another_owners_conversation() {
    let directory = temporary_directory();
    let store = ConversationStore::open(&directory).unwrap();
    let conversation = store.create("owner-a", "assistant").unwrap();

    assert!(matches!(
        store.delete("owner-b", conversation.id),
        Err(ConversationStoreError::OwnerMismatch { .. })
    ));
    assert!(store.get(conversation.id).is_ok());
    assert_eq!(
        store.active("owner-a").unwrap().unwrap().id,
        conversation.id
    );

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn saves_full_runtime_message_history() {
    let directory = temporary_directory();
    let store = ConversationStore::open(&directory).unwrap();
    let conversation = store.create("telegram:123:456", "assistant").unwrap();

    let messages = vec![
        Message::User {
            content: "Calculate 2 + 2".into(),
        },
        Message::Assistant {
            content: None,
            tool_calls: vec![ToolCall {
                id: "call-1".into(),
                name: "calculator".into(),
                arguments: json!({"expression": "2 + 2"}),
                provider_metadata: Some(json!({"provider": "test"})),
            }],
        },
        Message::Tool {
            tool_call_id: "call-1".into(),
            name: "calculator".into(),
            content: "4".into(),
            is_error: false,
            provider_metadata: None,
        },
    ];

    store
        .save_messages(conversation.id, messages.clone())
        .unwrap();
    let loaded = store.get(conversation.id).unwrap();

    assert_eq!(
        serde_json::to_value(loaded.messages).unwrap(),
        serde_json::to_value(messages).unwrap()
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn lists_only_conversations_for_the_requested_owner() {
    let directory = temporary_directory();
    let store = ConversationStore::open(&directory).unwrap();

    let first = store.create("owner-a", "assistant").unwrap();
    let other = store.create("owner-b", "assistant").unwrap();
    let second = store.create("owner-a", "assistant").unwrap();

    let conversations = store.list("owner-a").unwrap();
    assert_eq!(conversations.len(), 2);
    assert!(conversations.iter().any(|item| item.id == first.id));
    assert!(conversations.iter().any(|item| item.id == second.id));
    assert!(conversations.iter().all(|item| item.owner == "owner-a"));
    assert!(store.list("owner-c").unwrap().is_empty());
    assert_ne!(first.id, other.id);

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cannot_activate_another_owners_conversation() {
    let directory = temporary_directory();
    let store = ConversationStore::open(&directory).unwrap();
    let conversation = store.create("owner-a", "assistant").unwrap();

    assert!(matches!(
        store.set_active("owner-b", conversation.id),
        Err(ConversationStoreError::OwnerMismatch { .. })
    ));

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn new_conversation_replaces_active_without_deleting_previous_history() {
    let directory = temporary_directory();
    let store = ConversationStore::open(&directory).unwrap();

    let first = store.create("telegram:123:456", "assistant").unwrap();
    store
        .save_messages(
            first.id,
            vec![Message::User {
                content: "first session".into(),
            }],
        )
        .unwrap();

    let second = store.create("telegram:123:456", "assistant").unwrap();

    assert_eq!(
        store.active("telegram:123:456").unwrap().unwrap().id,
        second.id
    );
    assert_eq!(store.get(first.id).unwrap().messages.len(), 1);

    fs::remove_dir_all(directory).unwrap();
}
