//! The asynchronous contract implemented by run executors.

use super::ExecutionReport;
use crate::RunId;
use agent_runtime::{CancellationToken, Message, ToolContext};
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
}
