//! A run snapshot

use super::StoredRunStatus;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
