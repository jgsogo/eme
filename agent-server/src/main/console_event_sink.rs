//! Runtime event logging adapter for the server process.

use agent_runtime::{EventSink, RunEvent, RunEventKind};
use tracing::{debug, info, warn};

/// Sends runtime events to the server console as they happen.
pub(super) struct ConsoleEventSink;

impl EventSink for ConsoleEventSink {
    fn emit(&self, event: &RunEvent) {
        match &event.kind {
            RunEventKind::ModelRequested => {
                debug!(run_id = %event.run_id, iteration = event.iteration, "model request started")
            }
            RunEventKind::ModelCandidatesDiscovered { model_ids } => {
                info!(run_id = %event.run_id, iteration = event.iteration, candidates = ?model_ids, "compatible model candidates discovered")
            }
            RunEventKind::ModelCandidateAttemptStarted { model_id } => {
                info!(run_id = %event.run_id, iteration = event.iteration, model = %model_id, "model candidate attempt started")
            }
            RunEventKind::ModelCandidateAttemptFailed { model_id, category } => {
                warn!(run_id = %event.run_id, iteration = event.iteration, model = %model_id, failure_category = ?category, "model candidate attempt failed")
            }
            RunEventKind::ModelFallback {
                from_model_id,
                to_model_id,
                category,
            } => {
                info!(run_id = %event.run_id, iteration = event.iteration, from_model = %from_model_id, to_model = %to_model_id, failure_category = ?category, "falling back to next model candidate")
            }
            RunEventKind::ModelSelected { model_id } => {
                info!(run_id = %event.run_id, iteration = event.iteration, model = %model_id, "model selected")
            }
            RunEventKind::ModelRoutingFailed {
                attempted_candidates,
            } => {
                warn!(run_id = %event.run_id, iteration = event.iteration, attempted_candidates, "model routing failed")
            }
            RunEventKind::ModelResponded {
                model_id,
                tool_calls,
            } => {
                debug!(run_id = %event.run_id, iteration = event.iteration, model = %model_id, tool_calls, "model response received")
            }
            RunEventKind::ToolExecutionStarted { name, .. } => {
                info!(run_id = %event.run_id, iteration = event.iteration, tool = %name, "tool execution started")
            }
            RunEventKind::ToolExecutionFinished {
                name,
                error_category,
                ..
            } => {
                if let Some(category) = error_category {
                    warn!(run_id = %event.run_id, iteration = event.iteration, tool = %name, error_category = ?category, "tool execution failed");
                } else {
                    info!(run_id = %event.run_id, iteration = event.iteration, tool = %name, "tool execution finished");
                }
            }
            RunEventKind::ModelFailed { category } => {
                warn!(run_id = %event.run_id, iteration = event.iteration, error_category = ?category, "model request failed")
            }
            RunEventKind::Cancelled => {
                info!(run_id = %event.run_id, iteration = event.iteration, "runtime cancellation observed")
            }
            RunEventKind::TimedOut => {
                warn!(run_id = %event.run_id, iteration = event.iteration, "runtime timed out")
            }
        }
    }
}
