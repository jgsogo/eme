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
            RunEventKind::ModelResponded { tool_calls } => {
                debug!(run_id = %event.run_id, iteration = event.iteration, tool_calls, "model response received")
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
            RunEventKind::ModelFailed { category, error } => {
                warn!(run_id = %event.run_id, iteration = event.iteration, error_category = ?category, %error, "model request failed")
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
