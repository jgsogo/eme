use super::*;
use crate::{Conversation, ConversationId};
use agent_runtime::Message;
use chrono::Utc;
use std::{collections::HashMap, fs, path::PathBuf};
use uuid::Uuid;

fn temp_dir() -> PathBuf {
    std::env::temp_dir().join(format!("eme-legacy-import-{}", Uuid::new_v4()))
}

#[test]
fn imports_legacy_history_and_active_selection_idempotently() {
    let root = temp_dir();
    let conversations_dir = root.join("conversations");
    fs::create_dir_all(&conversations_dir).unwrap();

    let conversation = Conversation {
        id: ConversationId::new(),
        owner: "telegram:42".into(),
        agent: "assistant".into(),
        title: "Existing conversation".into(),
        archived: false,
        removed_at: None,
        selected_family_id: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        messages: vec![
            Message::User {
                content: "old prompt".into(),
            },
            Message::Assistant {
                content: Some("old answer".into()),
                tool_calls: Vec::new(),
                model_id: Some("provider/model".into()),
                generated_at: Some(Utc::now()),
            },
        ],
    };
    fs::write(
        conversations_dir.join(format!("{}.json", conversation.id)),
        serde_json::to_vec_pretty(&conversation).unwrap(),
    )
    .unwrap();

    let active = HashMap::from([(conversation.owner.clone(), conversation.id)]);
    fs::write(
        root.join("active-conversations.json"),
        serde_json::to_vec(&active).unwrap(),
    )
    .unwrap();

    let database = root.join("conversations.sqlite");
    let store = SqliteConversationStore::open(&database).unwrap();
    let first = store.import_legacy(&root).unwrap();
    assert_eq!(first.imported, 1);
    assert_eq!(first.already_present, 0);
    assert_eq!(first.active_mappings_imported, 1);
    assert!(first.invalid_documents.is_empty());
    assert!(first.invalid_active_entries.is_empty());

    let loaded = store.get(conversation.id).unwrap();
    assert_eq!(loaded.owner, conversation.owner);
    assert_eq!(loaded.agent, conversation.agent);
    assert_eq!(loaded.title, conversation.title);
    assert_eq!(
        serde_json::to_value(&loaded.messages).unwrap(),
        serde_json::to_value(&conversation.messages).unwrap(),
    );
    assert_eq!(
        store.active(&conversation.owner).unwrap().unwrap().id,
        conversation.id,
    );

    drop(store);
    let reopened = SqliteConversationStore::open(&database).unwrap();
    let second = reopened.import_legacy(&root).unwrap();
    assert_eq!(second.imported, 0);
    assert_eq!(second.already_present, 1);
    assert_eq!(second.active_mappings_imported, 0);
    assert_eq!(reopened.get(conversation.id).unwrap().messages.len(), 2);

    // Import is read-only with respect to the legacy source.
    assert!(
        conversations_dir
            .join(format!("{}.json", conversation.id))
            .exists()
    );
    assert!(root.join("active-conversations.json").exists());
    drop(reopened);
    let _ = fs::remove_dir_all(root);
}


#[test]
fn removing_conversation_preserves_history_and_hides_it_from_normal_access() {
    let root = temp_dir();
    fs::create_dir_all(&root).unwrap();
    let store = SqliteConversationStore::open(root.join("conversations.sqlite")).unwrap();
    let conversation = store.create("telegram:42", "assistant").unwrap();
    let messages = vec![Message::User { content: "keep this history".into() }];
    store.save_messages(conversation.id, messages.clone()).unwrap();

    store.delete("telegram:42", conversation.id).unwrap();

    assert!(matches!(store.get(conversation.id), Err(ConversationStoreError::NotFound(_))));
    assert!(store.list("telegram:42").unwrap().is_empty());
    assert!(store.active("telegram:42").unwrap().is_none());
    assert!(matches!(
        store.save_messages(conversation.id, Vec::new()),
        Err(ConversationStoreError::NotFound(_))
    ));

    let connection = store.lock();
    let removed_at: Option<String> = connection
        .query_row(
            "SELECT removed_at FROM conversations WHERE id = ?1",
            [conversation.id.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    let message_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM conversation_messages WHERE conversation_id = ?1",
            [conversation.id.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert!(removed_at.is_some());
    assert_eq!(message_count, messages.len() as i64);

    drop(connection);
    drop(store);
    let _ = fs::remove_dir_all(root);
}
