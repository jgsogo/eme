//! Adapts agent runtime execution to the service run executor contract.

use super::AgentRuntimeResolver;
use crate::{ExecutionOutcome, ExecutionReport, RunExecutor, RunId};
use agent_runtime::{
    CancellationToken, EventSink, Message, RunStatus as RuntimeRunStatus, ToolContext,
};
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

    async fn execute_inner(
        &self,
        run_id: RunId,
        agent_name: String,
        input: String,
        history: Vec<Message>,
        context: ToolContext,
        cancellation: CancellationToken,
        event_sink: Option<Arc<dyn EventSink>>,
    ) -> Result<ExecutionReport, String> {
        let resolved = self.resolver.resolve(&agent_name)?;
        let live_events = event_sink.is_some();
        let result = match event_sink {
            Some(event_sink) => {
                resolved
                    .runtime
                    .run_with_event_sink(
                        run_id.as_uuid(),
                        &resolved.agent,
                        context,
                        history,
                        input,
                        cancellation,
                        event_sink,
                    )
                    .await
            }
            None => {
                resolved
                    .runtime
                    .run(
                        run_id.as_uuid(),
                        &resolved.agent,
                        context,
                        history,
                        input,
                        cancellation,
                    )
                    .await
            }
        };

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
                    // With a live sink, events have already been delivered to the
                    // RunManager and must not be appended a second time.
                    runtime_events: if live_events {
                        Vec::new()
                    } else {
                        result.events
                    },
                    messages: Some(result.messages),
                })
            }
            Err(failure) => Ok(ExecutionReport {
                outcome: ExecutionOutcome::Failed(failure.to_string()),
                runtime_events: if live_events {
                    Vec::new()
                } else {
                    failure.events
                },
                messages: Some(failure.messages),
            }),
        }
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
        self.execute_inner(
            run_id,
            agent_name,
            input,
            history,
            context,
            cancellation,
            None,
        )
        .await
    }

    async fn execute_with_event_sink(
        &self,
        run_id: RunId,
        agent_name: String,
        input: String,
        history: Vec<Message>,
        context: ToolContext,
        cancellation: CancellationToken,
        event_sink: Arc<dyn EventSink>,
    ) -> Result<ExecutionReport, String> {
        self.execute_inner(
            run_id,
            agent_name,
            input,
            history,
            context,
            cancellation,
            Some(event_sink),
        )
        .await
    }
}
