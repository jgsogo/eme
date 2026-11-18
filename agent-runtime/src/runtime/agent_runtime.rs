//! Coordinates model turns, tool calls, authorization, cancellation, and run results.

use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

use super::{
    CancellationToken, EventSink, NoopEventSink, RunEvent, RunEventKind, RunFailure, RunResult,
    RunStatus, StopReason,
};
use crate::{
    Agent, AgentModel, Authorization, ErrorCategory, Message, ModelRequest, PolicyEngine,
    RuntimeError, ToolContext, ToolDispatcher, ToolError,
};

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

    /// Runs an agent with trusted platform context and optional conversation history.
    pub async fn run(
        &self,
        run_id: Uuid,
        agent: &Agent,
        mut context: ToolContext,
        mut messages: Vec<Message>,
        input: impl Into<String>,
        cancellation: CancellationToken,
    ) -> Result<RunResult, RunFailure> {
        context.run_id = Some(run_id);
        let deadline = self
            .timeout
            .map(|timeout| tokio::time::Instant::now() + timeout);

        let mut events = Vec::new();
        messages.push(Message::User {
            content: input.into(),
        });

        // Time is trusted host context, not model-authored conversation content.
        // Compute it for every run so every model turn receives the current date.
        let now = chrono::Utc::now();
        let date_context = format!(
            "Current date and time: {} (UTC). Current date: {} (UTC). Treat this as the authoritative current date and time; do not guess or infer it from conversation history.",
            now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            now.date_naive(),
        );
        let base_prompt = agent.system_prompt.as_deref().unwrap_or_default();
        let system_prompt = match context.family.as_ref() {
            Some(family) => {
                let family_json =
                    serde_json::to_string_pretty(family).expect("family context is serializable");
                Some(format!(
                    "{}\n\n{}\n\nTrusted context for the family selected in this conversation:\n{}\nUse these names and relationships as authoritative. Do not infer additional family members or relationships. If a name is unavailable, say so.",
                    base_prompt,
                    date_context,
                    family_json,
                ))
            }
            None => Some(format!("{}\n\n{}", base_prompt, date_context)),
        };

        let tools = match agent
            .tool_names
            .iter()
            .map(|name| {
                self.tools
                    .definition(name)
                    .ok_or_else(|| RuntimeError::Tool(ToolError::UnknownTool(name.clone())))
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
                system_prompt: system_prompt.clone(),
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
                                        category: ErrorCategory::Model,
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
                                    category: ErrorCategory::Model,
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
                        Authorization::Allowed => self
                            .tools
                            .execute(&context, &tool_name, arguments)
                            .await
                            .map_err(RuntimeError::Tool),
                        Authorization::Denied(error) => Err(RuntimeError::Authorization(error)),
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
                    Err(_) => Err(RuntimeError::Timeout(self.tool_timeout)),
                };

                self.emit_event(
                    &mut events,
                    RunEvent {
                        run_id,
                        iteration,
                        kind: RunEventKind::ToolExecutionFinished {
                            tool_call_id: call.id.clone(),
                            name: call.name.clone(),
                            error_category: result.as_ref().err().map(RuntimeError::category),
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

    use agent_models::ModelError;
    use async_trait::async_trait;
    use uuid::Uuid;

    use super::{AgentRuntime, CancellationToken, RunStatus};
    use crate::{
        Agent, AgentModel, AllowListPolicy, ModelRequest, ModelTurn, ToolContext, ToolRegistry,
    };

    struct FixedModel;

    #[async_trait]
    impl AgentModel for FixedModel {
        async fn complete(&self, _request: ModelRequest) -> Result<ModelTurn, ModelError> {
            Ok(ModelTurn {
                content: Some("completed".into()),
                tool_calls: Vec::new(),
            })
        }
    }

    #[tokio::test]
    async fn run_with_history_keeps_previous_messages_and_appends_new_input() {
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
        let history = vec![crate::Message::User {
            content: "previous turn".into(),
        }];

        let result = runtime
            .run(
                Uuid::new_v4(),
                &agent,
                ToolContext::default(),
                history,
                "current turn",
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert!(matches!(
            &result.messages[0],
            crate::Message::User { content } if content == "previous turn"
        ));
        assert!(matches!(
            &result.messages[1],
            crate::Message::User { content } if content == "current turn"
        ));
        assert!(matches!(
            &result.messages[2],
            crate::Message::Assistant { content: Some(content), .. } if content == "completed"
        ));
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
            .run(
                run_id,
                &agent,
                ToolContext::default(),
                Vec::new(),
                "hello",
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(result.run_id, run_id);
        assert_eq!(result.status, RunStatus::Completed);
        assert!(result.events.iter().all(|event| event.run_id == run_id));
    }
}

#[cfg(test)]
mod family_context_tests {
    use std::sync::{Arc, Mutex};

    use agent_models::ModelError;
    use async_trait::async_trait;
    use uuid::Uuid;

    use crate::{
        Agent, AgentModel, AgentRuntime, AllowListPolicy, CancellationToken, FamilyContext,
        FamilyMemberContext, ModelRequest, ModelTurn, ToolContext, ToolRegistry,
    };

    struct CapturingModel(Mutex<Option<ModelRequest>>);

    #[async_trait]
    impl AgentModel for CapturingModel {
        async fn complete(&self, request: ModelRequest) -> Result<ModelTurn, ModelError> {
            *self.0.lock().unwrap() = Some(request);
            Ok(ModelTurn {
                content: Some("done".into()),
                tool_calls: Vec::new(),
            })
        }
    }

    #[tokio::test]
    async fn selected_family_roster_is_injected_into_model_system_prompt() {
        let model = Arc::new(CapturingModel(Mutex::new(None)));
        let runtime = AgentRuntime::new(
            model.clone(),
            Arc::new(ToolRegistry::new()),
            Arc::new(AllowListPolicy::new(Vec::<String>::new())),
        );
        let agent = Agent {
            id: "assistant".into(),
            system_prompt: Some("You are a helpful assistant.".into()),
            tool_names: Vec::new(),
            max_iterations: 1,
        };
        let context = ToolContext {
            family: Some(FamilyContext {
                family_id: "family-1".into(),
                family_name: "Home".into(),
                family_role: "owner".into(),
                relationship: "father".into(),
                members: vec![
                    FamilyMemberContext {
                        user_id: "user-1".into(),
                        name: Some("Javier".into()),
                        birthday: None,
                        telegram_id: None,
                        role: "owner".into(),
                        relationship: "father".into(),
                    },
                    FamilyMemberContext {
                        user_id: "user-2".into(),
                        name: Some("Lucas".into()),
                        birthday: None,
                        telegram_id: None,
                        role: "member".into(),
                        relationship: "son".into(),
                    },
                ],
            }),
            ..ToolContext::default()
        };

        runtime
            .run(
                Uuid::new_v4(),
                &agent,
                context,
                Vec::new(),
                "Who is in my family?",
                CancellationToken::new(),
            )
            .await
            .unwrap();

        let request = model.0.lock().unwrap().take().unwrap();
        let prompt = request.system_prompt.unwrap();
        assert!(prompt.contains("Home"));
        assert!(prompt.contains("Javier"));
        assert!(prompt.contains("Lucas"));
        assert!(prompt.contains("father"));
        assert!(prompt.contains("son"));
        assert!(prompt.contains("Do not infer additional family members"));
        assert!(prompt.contains("Current date and time:"));
        assert!(prompt.contains(&chrono::Utc::now().date_naive().to_string()));
        assert!(!prompt.contains("user-1"));
    }
}
