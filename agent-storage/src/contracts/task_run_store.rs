//! Associates scheduled task definitions with durable agent runs.

use super::{StorageError, StoredRun};
use async_trait::async_trait;
use uuid::Uuid;

/// Associates a task occurrence with the run submitted for it.
///
/// Task definitions currently live in TOML files, so task IDs deliberately
/// have no foreign key: history survives deletion of a task definition.
#[async_trait]
pub trait TaskRunStore: Send + Sync {
    async fn associate_task_run(&self, task_id: Uuid, run_id: Uuid) -> Result<(), StorageError> {
        let _ = (task_id, run_id);
        Err(StorageError::Backend(
            "task execution history is not supported by this storage backend".into(),
        ))
    }

    async fn list_task_runs(&self, task_id: Uuid) -> Result<Vec<StoredRun>, StorageError> {
        let _ = task_id;
        Err(StorageError::Backend(
            "task execution history is not supported by this storage backend".into(),
        ))
    }

    async fn list_task_runs_page(&self, task_id: Uuid, limit: u64) -> Result<(Vec<StoredRun>, u64), StorageError> {
        let runs = self.list_task_runs(task_id).await?;
        let total = runs.len() as u64;
        Ok((runs.into_iter().take(limit as usize).collect(), total))
    }
}
