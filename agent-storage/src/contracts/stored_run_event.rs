//! An immutable event

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
