use super::*;
use crate::{RunStore, StoredRun, StoredRunStatus};
use agent_metering::{InvocationOutcome, MeteredValue, ModelPricing, UsageDimensions, UsageRecord};
use chrono::Utc;
use std::collections::BTreeMap;

fn run() -> StoredRun {
    StoredRun {
        id: Uuid::from_u128(42),
        conversation_id: None,
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

fn record(run_id: Uuid) -> UsageRecord {
    UsageRecord::new(
        Uuid::from_u128(43), run_id, 1, "assistant", Some("user-1".into()),
        "openai", "gpt-test", InvocationOutcome::Failed,
        UsageDimensions::new().with(UsageDimensions::INPUT_TOKENS, MeteredValue::Reported(100)),
        Utc::now(),
    ).unwrap()
}

#[tokio::test]
async fn usage_is_durable_and_idempotent_by_invocation_id() {
    let path = std::env::temp_dir().join(format!("eme-metering-{}.sqlite", Uuid::new_v4()));
    let _conversations = crate::SqliteConversationStore::open(&path).unwrap();
    let store = SqliteExecutionStore::open(&path).unwrap();
    let run = run();
    store.insert_run(run.clone()).await.unwrap();
    let record = record(run.id);
    store.append_usage(record.clone()).await.unwrap();
    store.append_usage(record.clone()).await.unwrap();
    assert_eq!(store.list_usage(Some(run.id)).await.unwrap(), vec![record.clone()]);
    drop(store);

    let reopened = SqliteExecutionStore::open(&path).unwrap();
    assert_eq!(reopened.list_usage(Some(run.id)).await.unwrap(), vec![record]);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn pricing_versions_are_immutable_and_queryable() {
    let path = std::env::temp_dir().join(format!("eme-pricing-{}.sqlite", Uuid::new_v4()));
    let store = SqliteExecutionStore::open(&path).unwrap();
    let pricing = ModelPricing {
        provider: "openai".into(),
        model: "gpt-test".into(),
        version: "2026-01".into(),
        currency: "USD".into(),
        effective_from: Utc::now(),
        effective_until: None,
        rates_per_million: BTreeMap::from([
            (UsageDimensions::INPUT_TOKENS.into(), Some(2_000_000)),
        ]),
    };
    store.insert_pricing(pricing.clone()).await.unwrap();
    assert_eq!(store.get_pricing("openai", "gpt-test", "2026-01").await.unwrap(), Some(pricing.clone()));
    assert_eq!(store.list_pricing("openai", "gpt-test").await.unwrap(), vec![pricing.clone()]);
    assert!(matches!(store.insert_pricing(pricing).await, Err(StorageError::AlreadyExists(_))));
    drop(store);
    let _ = std::fs::remove_file(path);
}
