use super::*;
use crate::{SqliteConversationStore, StoredRunStatus};
use agent_runtime::Message;
use chrono::Utc;
use std::{env, path::PathBuf};
use uuid::Uuid;

fn database_path() -> PathBuf {
    env::temp_dir().join(format!("eme-execution-{}.sqlite", Uuid::new_v4()))
}

#[tokio::test]
async fn persists_runs_events_and_structured_messages_across_reopen() {
    let path = database_path();
    let conversations = SqliteConversationStore::open(&path).unwrap();
    let conversation = conversations.create("owner", "assistant").unwrap();
    let conversation_id = Uuid::parse_str(&conversation.id.to_string()).unwrap();
    let store = SqliteExecutionStore::open(&path).unwrap();
    let run = StoredRun {
        id: Uuid::new_v4(),
        conversation_id: Some(conversation_id),
        agent: "assistant".into(),
        input: "hello".into(),
        status: StoredRunStatus::Queued,
        created_at: Utc::now(),
        started_at: None,
        finished_at: None,
        response: None,
        error: None,
    };
    store.insert_run(run.clone()).await.unwrap();
    store
        .append_event(StoredRunEvent {
            run_id: run.id,
            sequence: 0,
            kind: "queued".into(),
            iteration: None,
            tool_name: None,
            model_id: None,
            next_model_id: None,
            model_candidates: None,
            attempted_candidates: None,
            error_category: None,
            occurred_at: Utc::now(),
        })
        .await
        .unwrap();
    store
        .append_message(StoredMessage {
            conversation_id,
            run_id: Some(run.id),
            sequence: 0,
            message: Message::User {
                content: "hello".into(),
            },
            created_at: Utc::now(),
        })
        .await
        .unwrap();
    drop(store);

    let reopened = SqliteExecutionStore::open(&path).unwrap();
    assert_eq!(reopened.get_run(run.id).await.unwrap(), Some(run.clone()));
    assert_eq!(reopened.list_events(run.id, None).await.unwrap().len(), 1);
    assert_eq!(
        reopened.list_messages(conversation_id).await.unwrap().len(),
        1
    );
    let _ = std::fs::remove_file(&path);
}
