//! Transport- and backend-independent persistence contracts for agent execution.
//!
//! Implementations must preserve the ordering and uniqueness guarantees
//! documented by each trait. The contracts intentionally use platform-owned
//! records rather than depending on the service or HTTP crates.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use agent_runtime::Message;

/// A run snapshot suitable for durable storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredRun {
    pub id: Uuid,
    pub conversation_id: Option<Uuid>,
    pub agent: String,
    pub input: String,
    pub status: StoredRunStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub response: Option<String>,
    pub error: Option<String>,
}

/// Durable representation of a run's lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoredRunStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
    MaxIterationsReached,
}

/// An immutable event in a run's ordered event stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredRunEvent {
    pub run_id: Uuid,
    /// Zero-based, contiguous sequence number within the run.
    pub sequence: u64,
    /// Stable event name; payload fields remain explicit and versionable.
    pub kind: String,
    pub iteration: Option<usize>,
    pub tool_name: Option<String>,
    pub error_category: Option<String>,
    pub occurred_at: DateTime<Utc>,
}

/// A message in a conversation, optionally associated with the run that added it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub conversation_id: Uuid,
    pub run_id: Option<Uuid>,
    /// Zero-based, contiguous sequence number within the conversation.
    pub sequence: u64,
    pub message: Message,
    pub created_at: DateTime<Utc>,
}

/// Errors shared by storage contracts and their implementations.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum StorageError {
    #[error("record already exists: {0}")]
    AlreadyExists(String),
    #[error("record not found: {0}")]
    NotFound(String),
    #[error("invalid record association: {0}")]
    InvalidAssociation(String),
    #[error("sequence {sequence} already exists for {stream}")]
    DuplicateSequence { stream: String, sequence: u64 },
    #[error("expected sequence {expected} for {stream}, got {actual}")]
    OutOfOrderSequence {
        stream: String,
        expected: u64,
        actual: u64,
    },
}

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

/// Appends and reads ordered conversation messages.
///
/// Appends must be contiguous and start at sequence zero. Each message is
/// immutable once stored; corrections should be represented by a new message.
#[async_trait]
pub trait ConversationMessageStore: Send + Sync {
    async fn append_message(&self, message: StoredMessage) -> Result<(), StorageError>;
    async fn list_messages(
        &self,
        conversation_id: Uuid,
    ) -> Result<Vec<StoredMessage>, StorageError>;
}
