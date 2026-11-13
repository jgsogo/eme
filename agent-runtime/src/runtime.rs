use crate::{
    Agent, AgentModel, Authorization, Message, ModelRequest, PolicyEngine, ToolDispatcher,
    ToolError,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("Model request failed: {0}")]
    Model(String),

    #[error("Tool '{0}' is not registered")]
    UnknownTool(String),
}

#[derive(Debug, thiserror::Error)]
#[error("Agent run {run_id} failed: {error}")]
pub struct RunFailure {
    pub run_id: Uuid,
    #[source]
    pub error: RuntimeError,
    pub messages: Vec<Message>,
    pub events: Vec<RunEvent>,
    pub iterations: usize,
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
    Cancelled,
    TimedOut,
}

/// Shared cancellation signal for an agent run.
#[derive(Clone, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation of the run.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    /// Returns whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
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

    Cancelled,
    TimedOut,

    ModelFailed {
        error: String,
    },
}

#[derive(Debug, Clone, Copy)]
enum StopReason {
    Cancelled,
    TimedOut,
}

/// Receives runtime events as they occur.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: &RunEvent);
}

pub struct NoopEventSink;

impl EventSink for NoopEventSink {
    fn emit(&self, _event: &RunEvent) {}
}

/// An event sink that stores events in memory.
#[derive(Default)]
pub struct CollectingEventSink {
    events: Mutex<Vec<RunEvent>>,
}

impl CollectingEventSink {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a snapshot of all collected events.
    pub fn events(&self) -> Vec<RunEvent> {
        self.events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

impl EventSink for CollectingEventSink {
    fn emit(&self, event: &RunEvent) {
        self.events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(event.clone());
    }
}

pub struct AgentRuntime {
    model: Arc<dyn AgentModel>,
    tools: Arc<dyn ToolDispatcher>,
    policy: Arc<dyn PolicyEngine>,
    event_sink: Arc<dyn EventSink>,
    timeout: Option<Duration>,
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
            timeout: None,
        }
    }

    pub fn with_event_sink(mut self, event_sink: Arc<dyn EventSink>) -> Self {
        self.event_sink = event_sink;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    fn emit_event(&self, events: &mut Vec<RunEvent>, event: RunEvent) {
        self.event_sink.emit(&event);
        events.push(event);
    }

    fn stop_reason(
        &self,
        cancellation: &CancellationToken,
        deadline: Option<tokio::time::Instant>,
    ) -> Option<StopReason> {
        if cancellation.is_cancelled() {
            return Some(StopReason::Cancelled);
        }

        if deadline.is_some_and(|deadline| tokio::time::Instant::now() >= deadline) {
            return Some(StopReason::TimedOut);
        }

        None
    }

    fn stopped_result(
        &self,
        run_id: Uuid,
        iteration: usize,
        reason: StopReason,
        messages: Vec<Message>,
        events: &mut Vec<RunEvent>,
    ) -> RunResult {
        let (status, kind) = match reason {
            StopReason::Cancelled => (RunStatus::Cancelled, RunEventKind::Cancelled),
            StopReason::TimedOut => (RunStatus::TimedOut, RunEventKind::TimedOut),
        };

        self.emit_event(
            events,
            RunEvent {
                run_id,
                iteration,
                kind,
            },
        );

        RunResult {
            run_id,
            status,
            response: None,
            messages,
            iterations: iteration,
            events: events.clone(),
        }
    }

    pub async fn run(
        &self,
        agent: &Agent,
        input: impl Into<String>,
        cancellation: CancellationToken,
    ) -> Result<RunResult, RunFailure> {
        let run_id = Uuid::new_v4();
        let deadline = self
            .timeout
            .map(|timeout| tokio::time::Instant::now() + timeout);

        let mut events = Vec::new();
        let mut messages = vec![Message::User {
            content: input.into(),
        }];

        let tools = match agent
            .tool_names
            .iter()
            .map(|name| {
                self.tools
                    .definition(name)
                    .ok_or_else(|| RuntimeError::UnknownTool(name.clone()))
            })
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(tools) => tools,
            Err(error) => {
                return Err(RunFailure {
                    run_id,
                    error,
                    messages,
                    events,
                    iterations: 0,
                });
            }
        };

        for iteration in 1..=agent.max_iterations {
            let request = ModelRequest {
                system_prompt: agent.system_prompt.clone(),
                messages: messages.clone(),
                tools: tools.clone(),
            };

            if let Some(reason) = self.stop_reason(&cancellation, deadline) {
                return Ok(self.stopped_result(
                    run_id,
                    iteration - 1,
                    reason,
                    messages,
                    &mut events,
                ));
            }

            self.emit_event(
                &mut events,
                RunEvent {
                    run_id,
                    iteration,
                    kind: RunEventKind::ModelRequested,
                },
            );

            let turn = match deadline {
                Some(deadline) => {
                    match tokio::time::timeout_at(deadline, self.model.complete(request)).await {
                        Ok(Ok(turn)) => turn,
                        Ok(Err(error)) => {
                            let error = RuntimeError::Model(error);

                            self.emit_event(
                                &mut events,
                                RunEvent {
                                    run_id,
                                    iteration,
                                    kind: RunEventKind::ModelFailed {
                                        error: error.to_string(),
                                    },
                                },
                            );

                            return Err(RunFailure {
                                run_id,
                                error,
                                messages,
                                events,
                                iterations: iteration - 1,
                            });
                        }
                        Err(_) => {
                            return Ok(self.stopped_result(
                                run_id,
                                iteration - 1,
                                StopReason::TimedOut,
                                messages,
                                &mut events,
                            ));
                        }
                    }
                }
                None => match self.model.complete(request).await {
                    Ok(turn) => turn,
                    Err(error) => {
                        let error = RuntimeError::Model(error);

                        self.emit_event(
                            &mut events,
                            RunEvent {
                                run_id,
                                iteration,
                                kind: RunEventKind::ModelFailed {
                                    error: error.to_string(),
                                },
                            },
                        );

                        return Err(RunFailure {
                            run_id,
                            error,
                            messages,
                            events,
                            iterations: iteration - 1,
                        });
                    }
                },
            };

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
                if let Some(reason) = self.stop_reason(&cancellation, deadline) {
                    return Ok(self.stopped_result(
                        run_id,
                        iteration,
                        reason,
                        messages,
                        &mut events,
                    ));
                }

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

                let tool_name = call.name.clone();
                let arguments = call.arguments.clone();

                let execution = async {
                    match self.policy.authorize(agent, &call) {
                        Authorization::Allowed => self.tools.execute(&tool_name, arguments).await,
                        Authorization::Denied(reason) => Err(ToolError::Execution(format!(
                            "Tool execution denied: {reason}"
                        ))),
                    }
                };

                let result = match deadline {
                    Some(deadline) => match tokio::time::timeout_at(deadline, execution).await {
                        Ok(result) => result,
                        Err(_) => {
                            return Ok(self.stopped_result(
                                run_id,
                                iteration,
                                StopReason::TimedOut,
                                messages,
                                &mut events,
                            ));
                        }
                    },
                    None => execution.await,
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
                    provider_metadata: call.provider_metadata.clone(),
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
