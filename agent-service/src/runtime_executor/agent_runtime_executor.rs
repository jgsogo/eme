//! Adapts agent runtime execution to the service run executor contract.

use super::AgentRuntimeResolver;
use crate::{ExecutionOutcome, ExecutionReport, RunExecutor, RunId};
use agent_runtime::{CancellationToken, Message, RunStatus as RuntimeRunStatus, ToolContext};
use async_trait::async_trait;
use std::sync::Arc;

/// Adapts the agent runtime to the service's run executor contract.
pub struct AgentRuntimeExecutor {
    resolver: Arc<dyn AgentRuntimeResolver>,
}

impl AgentRuntimeExecutor {
    pub fn new(resolver: Arc<dyn AgentRuntimeResolver>) -> Self {
        Self { resolver }
    }
}

#[async_trait]
impl RunExecutor for AgentRuntimeExecutor {
    async fn execute(
        &self,
        run_id: RunId,
        agent_name: String,
        input: String,
        history: Vec<Message>,
        context: ToolContext,
        cancellation: CancellationToken,
    ) -> Result<ExecutionReport, String> {
        let resolved = self.resolver.resolve(&agent_name)?;
        let result = resolved
            .runtime
            .run(
                run_id.as_uuid(),
                &resolved.agent,
                context,
                history,
                input,
                cancellation,
            )
            .await;

        match result {
            Ok(result) => {
                let outcome = match result.status {
                    RuntimeRunStatus::Completed => ExecutionOutcome::Completed(result.response),
                    RuntimeRunStatus::Cancelled => ExecutionOutcome::Cancelled,
                    RuntimeRunStatus::TimedOut => ExecutionOutcome::TimedOut,
                    RuntimeRunStatus::MaxIterationsReached => {
                        ExecutionOutcome::MaxIterationsReached
                    }
                };
                Ok(ExecutionReport {
                    outcome,
                    runtime_events: result.events,
                    messages: Some(result.messages),
                })
            }
            Err(failure) => Ok(ExecutionReport {
                outcome: ExecutionOutcome::Failed(failure.to_string()),
                runtime_events: failure.events,
                messages: Some(failure.messages),
            }),
        }
    }
}
