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
    /// Stable error category; raw error messages are excluded.
    pub error_category: Option<&'static str>,
    pub occurred_at: SystemTime,
}
