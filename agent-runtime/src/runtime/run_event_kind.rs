//! Variants describing model routing, tool, timeout, and cancellation lifecycle events.

use crate::ErrorCategory;
use agent_metering::UsageRecord;
use agent_models::ModelFailureCategory;

#[derive(Debug, Clone)]
pub enum RunEventKind {
    ModelRequested,
    /// Durable usage and cost input for one model invocation.
    ModelUsageRecorded {
        record: UsageRecord,
    },
    ModelCandidatesDiscovered {
        model_ids: Vec<String>,
    },
    ModelCandidateAttemptStarted {
        model_id: String,
    },
    ModelCandidateAttemptFailed {
        model_id: String,
        category: ModelFailureCategory,
    },
    ModelFallback {
        from_model_id: String,
        to_model_id: String,
        category: ModelFailureCategory,
    },
    ModelSelected {
        model_id: String,
    },
    ModelRoutingFailed {
        attempted_candidates: usize,
    },
    ModelResponded {
        model_id: String,
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
    /// The model claimed to use a tool that was not dispatched during this run.
    HallucinatedToolInvocation {
        tool_name: String,
    },
    Cancelled,
    TimedOut,
    ModelFailed {
        category: ErrorCategory,
    },
}
