//! Events emitted during a run lifecycle.

use super::RunEventKind;
use std::time::SystemTime;

/// An immutable, ordered entry in a run's lifecycle history.
#[derive(Debug, Clone)]
pub struct RunEvent {
    /// Zero-based sequence number, monotonically increasing within a run.
    pub sequence: u64,
    pub kind: RunEventKind,
    /// Runtime iteration for model and tool events; absent for lifecycle events.
    pub iteration: Option<usize>,
    /// Tool name for tool execution events.
    pub tool_name: Option<String>,
    /// Provider-qualified model identity associated with a routing event.
    pub model_id: Option<String>,
    /// Next provider-qualified candidate selected for a fallback attempt.
    pub next_model_id: Option<String>,
    /// Deterministically ordered compatible model candidates.
    pub model_candidates: Option<Vec<String>>,
    /// Number of candidates attempted before routing terminated.
    pub attempted_candidates: Option<usize>,
    /// Stable error category; raw error messages are excluded.
    pub error_category: Option<&'static str>,
    pub occurred_at: SystemTime,
}
