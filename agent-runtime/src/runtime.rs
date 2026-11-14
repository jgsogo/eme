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

const DEFAULT_TOOL_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_MAX_TOOL_OUTPUT_BYTES: usize = 16 * 1024;
const TOOL_OUTPUT_TRUNCATED: &str = "\n[tool output truncated]";

fn limit_tool_output(mut output: String, max_bytes: usize) -> String {
    if output.len() <= max_bytes {
        return output;
    }

    let marker = if max_bytes >= TOOL_OUTPUT_TRUNCATED.len() {
        TOOL_OUTPUT_TRUNCATED
    } else {
        ""
    };
    let mut end = max_bytes.saturating_sub(marker.len()).min(output.len());
    while !output.is_char_boundary(end) {
        end -= 1;
    }
    output.truncate(end);
    output.push_str(marker);
    output
}

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
///
/// Cancellation is observed at operation boundaries. A model request or tool
/// invocation that has already started is allowed to finish; its result is
/// recorded before the runtime stops. No subsequent model or tool operation is
/// started after cancellation is observed. This avoids abandoning a tool result
/// when an external side effect may already have occurred.
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
    tool_timeout: Duration,
    max_tool_output_bytes: usize,
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
            tool_timeout: DEFAULT_TOOL_TIMEOUT,
            max_tool_output_bytes: DEFAULT_MAX_TOOL_OUTPUT_BYTES,
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

    /// Sets the maximum duration of an individual tool invocation.
    ///
    /// A timed-out tool future is dropped and its timeout is returned to the
    /// model as a tool error. This does not undo external side effects.
    pub fn with_tool_timeout(mut self, timeout: Duration) -> Self {
        self.tool_timeout = timeout;
        self
    }

    /// Sets the maximum number of UTF-8 bytes retained from each tool result.
    pub fn with_max_tool_output_bytes(mut self, max_bytes: usize) -> Self {
        self.max_tool_output_bytes = max_bytes;
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

    /// Runs an agent with a runtime-generated identifier.
    ///
    /// Callers that manage runs at a higher level should use run_with_id so the
    /// same identifier is shared across service, runtime, and event records.
    pub async fn run(
        &self,
        agent: &Agent,
        input: impl Into<String>,
        cancellation: CancellationToken,
    ) -> Result<RunResult, RunFailure> {
        self.run_with_id(Uuid::new_v4(), agent, input, cancellation)
            .await
    }

    /// Runs an agent using an identifier allocated by the calling service.
    pub async fn run_with_id(
        &self,
        run_id: Uuid,
        agent: &Agent,
        input: impl Into<String>,
        cancellation: CancellationToken,
    ) -> Result<RunResult, RunFailure> {
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

            // Preserve the completed model turn, but do not start any requested tools.
            if cancellation.is_cancelled() {
                return Ok(self.stopped_result(
                    run_id,
                    iteration,
                    StopReason::Cancelled,
                    messages,
                    &mut events,
                ));
            }

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

                let tool_deadline = tokio::time::Instant::now() + self.tool_timeout;
                let operation_deadline = deadline.map_or(tool_deadline, |run_deadline| {
                    run_deadline.min(tool_deadline)
                });

                let result = match tokio::time::timeout_at(operation_deadline, execution).await {
                    Ok(result) => result,
                    Err(_)
                        if deadline.is_some_and(|run_deadline| run_deadline <= tool_deadline) =>
                    {
                        return Ok(self.stopped_result(
                            run_id,
                            iteration,
                            StopReason::TimedOut,
                            messages,
                            &mut events,
                        ));
                    }
                    Err(_) => Err(ToolError::Timeout(self.tool_timeout)),
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
                let content = limit_tool_output(content, self.max_tool_output_bytes);

                messages.push(Message::Tool {
                    tool_call_id: call.id,
                    name: call.name,
                    content,
                    is_error,
                    provider_metadata: call.provider_metadata.clone(),
                });

                // The in-flight tool completed and its result is now part of the
                // conversation. Stop before starting another tool or model request.
                if cancellation.is_cancelled() {
                    return Ok(self.stopped_result(
                        run_id,
                        iteration,
                        StopReason::Cancelled,
                        messages,
                        &mut events,
                    ));
                }
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

#[cfg(test)]
mod external_run_id_tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use uuid::Uuid;

    use super::{AgentRuntime, CancellationToken, RunStatus};
    use crate::{Agent, AgentModel, AllowListPolicy, ModelRequest, ModelTurn, ToolRegistry};

    struct FixedModel;

    #[async_trait]
    impl AgentModel for FixedModel {
        async fn complete(&self, _request: ModelRequest) -> Result<ModelTurn, String> {
            Ok(ModelTurn {
                content: Some("completed".into()),
                tool_calls: Vec::new(),
            })
        }
    }

    #[tokio::test]
    async fn run_with_id_preserves_the_callers_identifier_in_result_and_events() {
        let runtime = AgentRuntime::new(
            Arc::new(FixedModel),
            Arc::new(ToolRegistry::new()),
            Arc::new(AllowListPolicy::new(Vec::<String>::new())),
        );
        let agent = Agent {
            id: "test-agent".into(),
            system_prompt: None,
            tool_names: Vec::new(),
            max_iterations: 2,
        };
        let run_id = Uuid::new_v4();

        let result = runtime
            .run_with_id(run_id, &agent, "hello", CancellationToken::new())
            .await
            .unwrap();

        assert_eq!(result.run_id, run_id);
        assert_eq!(result.status, RunStatus::Completed);
        assert!(result.events.iter().all(|event| event.run_id == run_id));
    }
}
