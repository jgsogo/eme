use super::*;
use crate::{RunStore, StoredRun, StoredRunStatus};
use chrono::Utc;

fn run(id: Uuid) -> StoredRun {
    StoredRun {
        id,
        conversation_id: None,
        agent: "assistant".into(),
        input: "private prompt".into(),
        status: StoredRunStatus::Queued,
        created_at: Utc::now(),
        started_at: None,
        finished_at: None,
        response: None,
        error: None,
    }
}

#[tokio::test]
async fn one_shot_owner_is_durable_and_cannot_be_reassigned() {
    let path = std::env::temp_dir().join(format!("eme-one-shot-owner-{}.sqlite", Uuid::new_v4()));
    let store = SqliteExecutionStore::open(&path).unwrap();
    let run_id = Uuid::new_v4();
    store.insert_run(run(run_id)).await.unwrap();

    store
        .associate_one_shot_run(run_id, "telegram:1001")
        .await
        .unwrap();
    store
        .associate_one_shot_run(run_id, "telegram:1001")
        .await
        .unwrap();
    assert!(matches!(
        store.associate_one_shot_run(run_id, "telegram:2002").await,
        Err(StorageError::InvalidAssociation(_))
    ));

    drop(store);
    let reopened = SqliteExecutionStore::open(&path).unwrap();
    assert_eq!(
        reopened.list_one_shot_run_owners().await.unwrap(),
        vec![(run_id.to_string(), "telegram:1001".into())]
    );
    drop(reopened);
    let _ = std::fs::remove_file(path);
}
