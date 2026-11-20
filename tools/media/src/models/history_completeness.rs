//! Confidence in whether local viewing history is sufficiently synchronized.
use serde::{Deserialize, Serialize};

/// Completeness is distinct from whether a particular title was watched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryCompleteness {
    Unknown,
    Partial,
    Complete,
}
