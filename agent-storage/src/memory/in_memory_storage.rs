//! Process-local implementation of execution persistence contracts.

use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::RwLock;
use uuid::Uuid;
use crate::{ConversationMessageStore, RunEventStore, RunStore, StorageError, StoredMessage, StoredRun, StoredRunEvent};
use super::State;

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
            .filter(|run| conversation_id.is_none_or(|id| run.conversation_id == Some(id)))
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
