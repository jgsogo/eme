use std::sync::Arc;

use agent_runtime::{Agent, AgentRuntime, CancellationToken, RunStatus as RuntimeRunStatus};
use async_trait::async_trait;

use crate::{ExecutionOutcome, ExecutionReport, RunExecutor, RunId};

/// An agent definition paired with the runtime configured to execute it.
pub struct ResolvedAgent {
    pub agent: Agent,
    pub runtime: Arc<AgentRuntime>,
}

/// Resolves an agent name to its definition and configured runtime.
///
/// The resolver is the composition boundary for agent definitions, model
/// clients, tool registries, and authorization policies. Its implementation
/// belongs in the application/bootstrap layer, not in the HTTP transport.
pub trait AgentRuntimeResolver: Send + Sync + 'static {
    fn resolve(&self, agent_name: &str) -> Result<ResolvedAgent, String>;
}

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
        cancellation: CancellationToken,
    ) -> Result<ExecutionOutcome, String> {
        self.execute_with_events(run_id, agent_name, input, cancellation)
            .await
            .map(|report| report.outcome)
    }

    async fn execute_with_events(
        &self,
        run_id: RunId,
        agent_name: String,
        input: String,
        cancellation: CancellationToken,
    ) -> Result<ExecutionReport, String> {
        let resolved = self.resolver.resolve(&agent_name)?;
        let result = resolved
            .runtime
            .run_with_id(run_id.as_uuid(), &resolved.agent, input, cancellation)
            .await;

        let result = match result {
            Ok(result) => result,
            Err(failure) => {
                return Ok(ExecutionReport {
                    outcome: ExecutionOutcome::Failed(failure.to_string()),
                    runtime_events: failure.events,
                });
            }
        };

        let outcome = match result.status {
            RuntimeRunStatus::Completed => ExecutionOutcome::Completed(result.response),
            RuntimeRunStatus::Cancelled => ExecutionOutcome::Cancelled,
            RuntimeRunStatus::TimedOut => ExecutionOutcome::TimedOut,
            RuntimeRunStatus::MaxIterationsReached => ExecutionOutcome::MaxIterationsReached,
        };

        Ok(ExecutionReport {
            outcome,
            runtime_events: result.events,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use agent_runtime::{
        Agent, AgentModel, AgentRuntime, AllowListPolicy, ModelError, ModelRequest, ModelTurn,
        ToolRegistry,
    };
    use async_trait::async_trait;

    use super::{AgentRuntimeExecutor, AgentRuntimeResolver, ResolvedAgent};
    use crate::{ExecutionOutcome, RunExecutor, RunId};

    struct FixedModel;

    #[async_trait]
    impl AgentModel for FixedModel {
        async fn complete(&self, _request: ModelRequest) -> Result<ModelTurn, ModelError> {
            Ok(ModelTurn {
                content: Some("runtime response".into()),
                tool_calls: Vec::new(),
            })
        }
    }

    struct FixedResolver {
        runtime: Arc<AgentRuntime>,
    }

    impl AgentRuntimeResolver for FixedResolver {
        fn resolve(&self, agent_name: &str) -> Result<ResolvedAgent, String> {
            if agent_name != "assistant" {
                return Err(format!("unknown agent '{agent_name}'"));
            }

            Ok(ResolvedAgent {
                agent: Agent {
                    id: agent_name.to_owned(),
                    system_prompt: Some("Be helpful.".into()),
                    tool_names: Vec::new(),
                    max_iterations: 3,
                },
                runtime: Arc::clone(&self.runtime),
            })
        }
    }

    fn executor() -> AgentRuntimeExecutor {
        let runtime = Arc::new(AgentRuntime::new(
            Arc::new(FixedModel),
            Arc::new(ToolRegistry::new()),
            Arc::new(AllowListPolicy::new(Vec::<String>::new())),
        ));

        AgentRuntimeExecutor::new(Arc::new(FixedResolver { runtime }))
    }

    #[tokio::test]
    async fn executes_resolved_agent_and_returns_runtime_response() {
        let executor = executor();

        let outcome = executor
            .execute(
                RunId::new(),
                "assistant".into(),
                "hello".into(),
                agent_runtime::CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(
            outcome,
            ExecutionOutcome::Completed(Some("runtime response".into()))
        );
    }

    #[tokio::test]
    async fn reports_unknown_agents_as_execution_errors() {
        let executor = executor();

        let error = executor
            .execute(
                RunId::new(),
                "missing".into(),
                "hello".into(),
                agent_runtime::CancellationToken::new(),
            )
            .await
            .unwrap_err();

        assert_eq!(error, "unknown agent 'missing'");
    }
}
