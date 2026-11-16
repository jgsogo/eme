//! Stores mutable

use super::{StorageError, StoredRun};
use async_trait::async_trait;
use uuid::Uuid;

/// Stores mutable run snapshots.
///
/// insert_run must reject an existing identifier. update_run must reject a
/// missing identifier and must not change the record's identity. list_runs
/// returns snapshots ordered newest-first by creation time.
#[async_trait]
pub trait RunStore: Send + Sync {
    async fn insert_run(&self, run: StoredRun) -> Result<(), StorageError>;
    async fn update_run(&self, run: StoredRun) -> Result<(), StorageError>;
    async fn get_run(&self, id: Uuid) -> Result<Option<StoredRun>, StorageError>;
    async fn list_runs(
        &self,
        conversation_id: Option<Uuid>,
    ) -> Result<Vec<StoredRun>, StorageError>;
}
