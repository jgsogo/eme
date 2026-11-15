use super::{ExecutionOutcome, ExecutionReport};
use crate::RunId;
use agent_runtime::{CancellationToken, Message};
use async_trait::async_trait;

/// Executes the work associated with an accepted run.
///
/// Implementations adapt the platform's agent runtime to this service contract.
#[async_trait]
pub trait RunExecutor: Send + Sync + 'static {
    async fn execute(
        &self,
        run_id: RunId,
        agent: String,
        input: String,
        cancellation: CancellationToken,
    ) -> Result<ExecutionOutcome, String>;

    async fn execute_with_events(
        &self,
        run_id: RunId,
        agent: String,
        input: String,
        cancellation: CancellationToken,
    ) -> Result<ExecutionReport, String> {
        self.execute(run_id, agent, input, cancellation)
            .await
            .map(|outcome| ExecutionReport {
                outcome,
                runtime_events: Vec::new(),
                messages: None,
            })
    }

    async fn execute_with_history(
        &self,
        run_id: RunId,
        agent: String,
        input: String,
        history: Vec<Message>,
        cancellation: CancellationToken,
    ) -> Result<ExecutionReport, String> {
        if !history.is_empty() {
            return Err("this run executor does not support conversation history".into());
        }

        self.execute_with_events(run_id, agent, input, cancellation)
            .await
    }
}
