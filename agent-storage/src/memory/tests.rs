//! Tests for in-memory execution persistence.

use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use crate::contracts::{
    ConversationMessageStore, RunEventStore, RunStore, StorageError, StoredMessage, StoredRun,
    StoredRunEvent, StoredRunStatus,
};

use super::InMemoryStorage;
use agent_runtime::Message;

fn run(id: Uuid, conversation_id: Option<Uuid>) -> StoredRun {
    StoredRun {
        id,
        conversation_id,
        agent: "assistant".into(),
        input: "hello".into(),
        status: StoredRunStatus::Queued,
        created_at: Utc::now(),
        started_at: None,
        finished_at: None,
        response: None,
        error: None,
    }
}

#[tokio::test]
async fn run_store_enforces_identity_and_lists_by_conversation() {
    let store = InMemoryStorage::default();
    let conversation_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    store
        .insert_run(run(run_id, Some(conversation_id)))
        .await
        .unwrap();

    assert!(matches!(
        store.insert_run(run(run_id, Some(conversation_id))).await,
        Err(StorageError::AlreadyExists(_))
    ));
    assert_eq!(
        store.list_runs(Some(conversation_id)).await.unwrap().len(),
        1
    );
    assert!(store.get_run(run_id).await.unwrap().is_some());

    let mut updated = store.get_run(run_id).await.unwrap().unwrap();
    updated.status = StoredRunStatus::Completed;
    updated.response = Some("hi".into());
    store.update_run(updated).await.unwrap();
    assert_eq!(
        store.get_run(run_id).await.unwrap().unwrap().status,
        StoredRunStatus::Completed
    );
    assert!(matches!(
        store.update_run(run(Uuid::new_v4(), None)).await,
        Err(StorageError::NotFound(_))
    ));
}

#[tokio::test]
async fn event_store_enforces_contiguous_sequences_and_supports_cursors() {
    let store = InMemoryStorage::default();
    let run_id = Uuid::new_v4();
    store.insert_run(run(run_id, None)).await.unwrap();

    let event = |sequence| StoredRunEvent {
        run_id,
        sequence,
        kind: "started".into(),
        iteration: None,
        tool_name: None,
        error_category: None,
        occurred_at: Utc::now(),
    };

    assert!(matches!(
        store.append_event(event(1)).await,
        Err(StorageError::OutOfOrderSequence { expected: 0, .. })
    ));
    store.append_event(event(0)).await.unwrap();
    assert!(matches!(
        store.append_event(event(0)).await,
        Err(StorageError::DuplicateSequence { .. })
    ));
    store.append_event(event(1)).await.unwrap();
    assert_eq!(store.list_events(run_id, Some(0)).await.unwrap().len(), 1);
    assert_eq!(store.list_events(run_id, None).await.unwrap().len(), 2);
}

#[tokio::test]
async fn message_store_preserves_order_and_runtime_message_payloads() {
    let store = InMemoryStorage::default();
    let conversation_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    store
        .insert_run(run(run_id, Some(conversation_id)))
        .await
        .unwrap();

    let message = StoredMessage {
        conversation_id,
        run_id: Some(run_id),
        sequence: 0,
        message: Message::Assistant {
            content: Some("hello".into()),
            tool_calls: Vec::new(),
        },
        created_at: Utc::now(),
    };
    store.append_message(message).await.unwrap();
    assert_eq!(store.list_messages(conversation_id).await.unwrap().len(), 1);

    let invalid = StoredMessage {
        conversation_id,
        run_id: Some(Uuid::new_v4()),
        sequence: 1,
        message: Message::User {
            content: json!({"text": "ignored"}).to_string(),
        },
        created_at: Utc::now(),
    };
    assert!(matches!(
        store.append_message(invalid).await,
        Err(StorageError::NotFound(_))
    ));

    let other_conversation = Uuid::new_v4();
    let other_run = Uuid::new_v4();
    store
        .insert_run(run(other_run, Some(other_conversation)))
        .await
        .unwrap();
    let mismatched = StoredMessage {
        conversation_id,
        run_id: Some(other_run),
        sequence: 1,
        message: Message::User {
            content: "wrong conversation".into(),
        },
        created_at: Utc::now(),
    };
    assert!(matches!(
        store.append_message(mismatched).await,
        Err(StorageError::InvalidAssociation(_))
    ));
}
