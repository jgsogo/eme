use std::sync::Arc;

use crate::{
    Agent, AgentModel, Authorization, Message, ModelRequest, PolicyEngine, ToolDispatcher,
    ToolError,
};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("Model request failed: {0}")]
    Model(String),

    #[error("Tool '{0}' is not registered")]
    UnknownTool(String),
}

pub struct RunResult {
    pub run_id: Uuid,
    pub status: RunStatus,
    pub response: Option<String>,
    pub messages: Vec<Message>,
    pub iterations: usize,
    pub events: Vec<RunEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunStatus {
    Completed,
    MaxIterationsReached,
}

#[derive(Debug, Clone)]
pub struct RunEvent {
    pub run_id: Uuid,
    pub iteration: usize,
    pub kind: RunEventKind,
}

#[derive(Debug, Clone)]
pub enum RunEventKind {
    ModelRequested,

    ModelResponded {
        tool_calls: usize,
    },

    ToolExecutionStarted {
        tool_call_id: String,
        name: String,
    },

    ToolExecutionFinished {
        tool_call_id: String,
        name: String,
        is_error: bool,
    },
}

/// Receives runtime events as they occur.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: &RunEvent);
}

pub struct NoopEventSink;

impl EventSink for NoopEventSink {
    fn emit(&self, _event: &RunEvent) {}
}

pub struct AgentRuntime {
    model: Arc<dyn AgentModel>,
    tools: Arc<dyn ToolDispatcher>,
    policy: Arc<dyn PolicyEngine>,
    event_sink: Arc<dyn EventSink>,
}

impl AgentRuntime {
    pub fn new(
        model: Arc<dyn AgentModel>,
        tools: Arc<dyn ToolDispatcher>,
        policy: Arc<dyn PolicyEngine>,
    ) -> Self {
        Self {
            model,
            tools,
            policy,
            event_sink: Arc::new(NoopEventSink),
        }
    }

    pub fn with_event_sink(mut self, event_sink: Arc<dyn EventSink>) -> Self {
        self.event_sink = event_sink;
        self
    }

    fn emit_event(&self, events: &mut Vec<RunEvent>, event: RunEvent) {
        self.event_sink.emit(&event);
        events.push(event);
    }

    pub async fn run(
        &self,
        agent: &Agent,
        input: impl Into<String>,
    ) -> Result<RunResult, RuntimeError> {
        let run_id = Uuid::new_v4();
        let mut events = Vec::new();

        let tools = agent
            .tool_names
            .iter()
            .map(|name| {
                self.tools
                    .definition(name)
                    .ok_or_else(|| RuntimeError::UnknownTool(name.clone()))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut messages = vec![Message::User {
            content: input.into(),
        }];

        for iteration in 1..=agent.max_iterations {
            let request = ModelRequest {
                system_prompt: agent.system_prompt.clone(),
                messages: messages.clone(),
                tools: tools.clone(),
            };

            self.emit_event(
                &mut events,
                RunEvent {
                    run_id,
                    iteration,
                    kind: RunEventKind::ModelRequested,
                },
            );

            let turn = self
                .model
                .complete(request)
                .await
                .map_err(RuntimeError::Model)?;

            self.emit_event(
                &mut events,
                RunEvent {
                    run_id,
                    iteration,
                    kind: RunEventKind::ModelResponded {
                        tool_calls: turn.tool_calls.len(),
                    },
                },
            );

            messages.push(Message::Assistant {
                content: turn.content.clone(),
                tool_calls: turn.tool_calls.clone(),
            });

            if turn.tool_calls.is_empty() {
                return Ok(RunResult {
                    run_id,
                    status: RunStatus::Completed,
                    response: turn.content,
                    messages,
                    iterations: iteration,
                    events,
                });
            }

            for call in turn.tool_calls {
                self.emit_event(
                    &mut events,
                    RunEvent {
                        run_id,
                        iteration,
                        kind: RunEventKind::ToolExecutionStarted {
                            tool_call_id: call.id.clone(),
                            name: call.name.clone(),
                        },
                    },
                );

                let result = match self.policy.authorize(agent, &call) {
                    Authorization::Allowed => self.tools.execute(&call.name, call.arguments).await,
                    Authorization::Denied(reason) => Err(ToolError::Execution(format!(
                        "Tool execution denied: {reason}"
                    ))),
                };

                self.emit_event(
                    &mut events,
                    RunEvent {
                        run_id,
                        iteration,
                        kind: RunEventKind::ToolExecutionFinished {
                            tool_call_id: call.id.clone(),
                            name: call.name.clone(),
                            is_error: result.is_err(),
                        },
                    },
                );

                let (content, is_error) = match result {
                    Ok(output) => (output, false),
                    Err(error) => (error.to_string(), true),
                };

                messages.push(Message::Tool {
                    tool_call_id: call.id,
                    name: call.name,
                    content,
                    is_error,
                });
            }
        }

        Ok(RunResult {
            run_id,
            status: RunStatus::MaxIterationsReached,
            response: None,
            messages,
            iterations: agent.max_iterations,
            events,
        })
    }
}
