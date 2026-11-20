//! An immutable event in a run's ordered event stream.

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
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub next_model_id: Option<String>,
    #[serde(default)]
    pub model_candidates: Option<Vec<String>>,
    #[serde(default)]
    pub attempted_candidates: Option<usize>,
    pub error_category: Option<String>,
    pub occurred_at: DateTime<Utc>,
}
