//! In-memory implementation of the storage contracts.
//!
//! This backend is intended for tests and local composition. It provides
//! process-local consistency but does not survive process restarts.

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::contracts::{
    ConversationMessageStore, RunEventStore, RunStore, StorageError, StoredMessage, StoredRun,
    StoredRunEvent,
};

#[derive(Default)]
struct State {
    runs: HashMap<Uuid, StoredRun>,
    events: HashMap<Uuid, Vec<StoredRunEvent>>,
    messages: HashMap<Uuid, Vec<StoredMessage>>,
}

/// A cloneable in-memory store implementing all execution storage contracts.
#[derive(Clone, Default)]
pub struct InMemoryStorage {
    state: Arc<RwLock<State>>,
}

#[async_trait]
impl RunStore for InMemoryStorage {
    async fn insert_run(&self, run: StoredRun) -> Result<(), StorageError> {
        let mut state = self.state.write().await;
        if state.runs.contains_key(&run.id) {
            return Err(StorageError::AlreadyExists(format!("run {}", run.id)));
        }
        state.runs.insert(run.id, run);
        Ok(())
    }

    async fn update_run(&self, run: StoredRun) -> Result<(), StorageError> {
        let mut state = self.state.write().await;
        if !state.runs.contains_key(&run.id) {
            return Err(StorageError::NotFound(format!("run {}", run.id)));
        }
        state.runs.insert(run.id, run);
        Ok(())
    }

    async fn get_run(&self, id: Uuid) -> Result<Option<StoredRun>, StorageError> {
        Ok(self.state.read().await.runs.get(&id).cloned())
    }

    async fn list_runs(
        &self,
        conversation_id: Option<Uuid>,
    ) -> Result<Vec<StoredRun>, StorageError> {
        let state = self.state.read().await;
        let mut runs: Vec<_> = state
            .runs
            .values()
            .filter(|run| {
                conversation_id.is_none_or(|id| run.conversation_id == Some(id))
            })
            .cloned()
            .collect();
        runs.sort_by(|left, right| {
            right
                .created_at
                .cmp(&left.created_at)
                .then_with(|| right.id.cmp(&left.id))
        });
        Ok(runs)
    }
}

#[async_trait]
impl RunEventStore for InMemoryStorage {
    async fn append_event(&self, event: StoredRunEvent) -> Result<(), StorageError> {
        let mut state = self.state.write().await;
        if !state.runs.contains_key(&event.run_id) {
            return Err(StorageError::NotFound(format!("run {}", event.run_id)));
        }

        let events = state.events.entry(event.run_id).or_default();
        let expected = events.len() as u64;
        if event.sequence < expected {
            return Err(StorageError::DuplicateSequence {
                stream: format!("run {}", event.run_id),
                sequence: event.sequence,
            });
        }
        if event.sequence > expected {
            return Err(StorageError::OutOfOrderSequence {
                stream: format!("run {}", event.run_id),
                expected,
                actual: event.sequence,
            });
        }
        events.push(event);
        Ok(())
    }

    async fn list_events(
        &self,
        run_id: Uuid,
        after_sequence: Option<u64>,
    ) -> Result<Vec<StoredRunEvent>, StorageError> {
        let state = self.state.read().await;
        if !state.runs.contains_key(&run_id) {
            return Err(StorageError::NotFound(format!("run {run_id}")));
        }
        Ok(state
            .events
            .get(&run_id)
            .into_iter()
            .flatten()
            .filter(|event| after_sequence.is_none_or(|cursor| event.sequence > cursor))
            .cloned()
            .collect())
    }
}

#[async_trait]
impl ConversationMessageStore for InMemoryStorage {
    async fn append_message(&self, message: StoredMessage) -> Result<(), StorageError> {
        let mut state = self.state.write().await;
        if let Some(run_id) = message.run_id {
            let Some(run) = state.runs.get(&run_id) else {
                return Err(StorageError::NotFound(format!("run {run_id}")));
            };
            if run.conversation_id != Some(message.conversation_id) {
                return Err(StorageError::InvalidAssociation(format!(
                    "run {run_id} does not belong to conversation {}",
                    message.conversation_id
                )));
            }
        }

        let messages = state.messages.entry(message.conversation_id).or_default();
        let expected = messages.len() as u64;
        if message.sequence < expected {
            return Err(StorageError::DuplicateSequence {
                stream: format!("conversation {}", message.conversation_id),
                sequence: message.sequence,
            });
        }
        if message.sequence > expected {
            return Err(StorageError::OutOfOrderSequence {
                stream: format!("conversation {}", message.conversation_id),
                expected,
                actual: message.sequence,
            });
        }
        messages.push(message);
        Ok(())
    }

    async fn list_messages(
        &self,
        conversation_id: Uuid,
    ) -> Result<Vec<StoredMessage>, StorageError> {
        Ok(self
            .state
            .read()
            .await
            .messages
            .get(&conversation_id)
            .cloned()
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
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
            store
                .list_runs(Some(conversation_id))
                .await
                .unwrap()
                .len(),
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
        store.insert_run(run(run_id, Some(conversation_id))).await.unwrap();

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
    }
}
