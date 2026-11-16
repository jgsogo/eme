//! Appends and reads immutable

use super::{StorageError, StoredRunEvent};
use async_trait::async_trait;
use uuid::Uuid;

/// Appends and reads immutable run events.
///
/// Appends must be contiguous and start at sequence zero. A duplicate or
/// out-of-order sequence is rejected. Reads are returned in ascending sequence
/// order; after_sequence is an exclusive cursor for reconnectable consumers.
#[async_trait]
pub trait RunEventStore: Send + Sync {
    async fn append_event(&self, event: StoredRunEvent) -> Result<(), StorageError>;
    async fn list_events(
        &self,
        run_id: Uuid,
        after_sequence: Option<u64>,
    ) -> Result<Vec<StoredRunEvent>, StorageError>;
}
