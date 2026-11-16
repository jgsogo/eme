//! Variants describing model, tool, timeout, and cancellation lifecycle events.

use crate::ErrorCategory;

#[derive(Debug, Clone)]
pub enum RunEventKind {
    ModelRequested,

    ModelResponded {
        tool_calls: usize,
    },

    ToolExecutionStarted {
        tool_call_id: String,
        name: String,
    },

    ToolExecutionFinished {
        tool_call_id: String,
        name: String,
        error_category: Option<ErrorCategory>,
    },

    Cancelled,
    TimedOut,

    ModelFailed {
        category: ErrorCategory,
        error: String,
    },
}
