//! Tests for runtime executor integration.

use std::sync::Arc;

use agent_runtime::{
    Agent, AgentModel, AgentRuntime, AllowListPolicy, ModelError, ModelRequest, ModelTurn,
    ToolContext, ToolRegistry,
};
use async_trait::async_trait;

use super::{AgentRuntimeExecutor, AgentRuntimeResolver};
use crate::ResolvedAgent;
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
                capabilities: Vec::new(),
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

    let report = executor
        .execute(
            RunId::new(),
            "assistant".into(),
            "hello".into(),
            Vec::new(),
            ToolContext::default(),
            agent_runtime::CancellationToken::new(),
        )
        .await
        .unwrap();

    assert_eq!(
        report.outcome,
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
            Vec::new(),
            ToolContext::default(),
            agent_runtime::CancellationToken::new(),
        )
        .await
        .unwrap_err();

    assert_eq!(error, "unknown agent 'missing'");
}
