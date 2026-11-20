//! The asynchronous contract implemented by run executors.

use super::ExecutionReport;
use crate::RunId;
use agent_runtime::{CancellationToken, EventSink, Message, ToolContext};
use std::sync::Arc;
use async_trait::async_trait;

/// Executes an accepted agent run with its conversation history and trusted context.
#[async_trait]
pub trait RunExecutor: Send + Sync + 'static {
    async fn execute(
        &self,
        run_id: RunId,
        agent: String,
        input: String,
        history: Vec<Message>,
        context: ToolContext,
        cancellation: CancellationToken,
    ) -> Result<ExecutionReport, String>;

    /// Executes while forwarding runtime events as they occur. Executors that
    /// do not support live events may keep the default batch-based behavior.
    async fn execute_with_event_sink(
        &self,
        run_id: RunId,
        agent: String,
        input: String,
        history: Vec<Message>,
        context: ToolContext,
        cancellation: CancellationToken,
        event_sink: Arc<dyn EventSink>,
    ) -> Result<ExecutionReport, String> {
        drop(event_sink);
        self.execute(run_id, agent, input, history, context, cancellation).await
    }
}
