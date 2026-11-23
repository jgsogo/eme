//! Coordinates model turns, tool calls, authorization, cancellation, and run results.

use std::sync::Arc;

use agent_metering::{InvocationOutcome, MeteredValue, ReportedCost, UsageDimensions, UsageRecord};
use agent_models::TokenUsage;
use std::time::Duration;
use uuid::Uuid;

use super::{
    CancellationToken, EventSink, NoopEventSink, RunEvent, RunEventKind, RunFailure, RunResult,
    RunStatus, StopReason,
};
use crate::{
    Agent, AgentModel, Authorization, ErrorCategory, FixedModelRouter, Message, ModelError,
    ModelRequest, ModelRouter, ModelRoutingEvent, ModelRoutingEventSink, PolicyEngine,
    RoutedModelTurn, RuntimeError, ToolContext, ToolDispatcher, ToolError,
};
use tokio::sync::mpsc;

const DEFAULT_TOOL_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_MAX_TOOL_OUTPUT_BYTES: usize = 16 * 1024;
const TOOL_OUTPUT_TRUNCATED: &str = "\n[tool output truncated]";
struct ModelRoutingEventChannel(mpsc::UnboundedSender<ModelRoutingEvent>);

impl ModelRoutingEventSink for ModelRoutingEventChannel {
    fn emit(&self, event: ModelRoutingEvent) {
        let _ = self.0.send(event);
    }
}

struct CompositeEventSink {
    sinks: Vec<Arc<dyn EventSink>>,
}

impl EventSink for CompositeEventSink {
    fn emit(&self, event: &RunEvent) {
        for sink in &self.sinks {
            sink.emit(event);
        }
    }
}

enum ModelTurnError {
    Model(ModelError),
    TimedOut,
}

fn usage_dimensions(usage: Option<&TokenUsage>) -> UsageDimensions {
    let Some(usage) = usage else {
        return UsageDimensions::new();
    };
    let mut dimensions = UsageDimensions::new()
        .with(
            UsageDimensions::INPUT_TOKENS,
            MeteredValue::Reported(u64::from(usage.input_tokens)),
        )
        .with(
            UsageDimensions::OUTPUT_TOKENS,
            MeteredValue::Reported(u64::from(usage.output_tokens)),
        );
    if let Some(value) = usage.cached_input_tokens {
        dimensions = dimensions.with(
            UsageDimensions::CACHED_INPUT_TOKENS,
            MeteredValue::Reported(u64::from(value)),
        );
    }
    if let Some(value) = usage.reasoning_tokens {
        dimensions = dimensions.with(
            UsageDimensions::REASONING_TOKENS,
            MeteredValue::Reported(u64::from(value)),
        );
    }
    dimensions
}

fn usage_record(
    run_id: Uuid,
    iteration: usize,
    agent: &str,
    principal_id: Option<String>,
    model_id: &str,
    outcome: InvocationOutcome,
    usage: UsageDimensions,
) -> UsageRecord {
    let (provider, model) = model_id.split_once('/').unwrap_or(("unknown", model_id));
    UsageRecord::new(
        Uuid::new_v4(),
        run_id,
        u32::try_from(iteration).unwrap_or(u32::MAX),
        agent,
        principal_id,
        provider,
        model,
        outcome,
        usage,
        chrono::Utc::now(),
    )
    .expect("runtime attribution is non-empty")
}

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

#[derive(Clone)]
pub struct AgentRuntime {
    model_router: Arc<dyn ModelRouter>,
    tools: Arc<dyn ToolDispatcher>,
    policy: Arc<dyn PolicyEngine>,
    event_sink: Arc<dyn EventSink>,
    timeout: Option<Duration>,
    tool_timeout: Duration,
    max_tool_output_bytes: usize,
}

impl AgentRuntime {
    /// Creates a runtime using a fixed model (useful for tests and embedding).
    pub fn new(
        model: Arc<dyn AgentModel>,
        tools: Arc<dyn ToolDispatcher>,
        policy: Arc<dyn PolicyEngine>,
    ) -> Self {
        Self::new_with_router(Arc::new(FixedModelRouter::new(model)), tools, policy)
    }

    /// Creates a runtime whose model is selected independently for each turn.
    pub fn new_with_router(
        model_router: Arc<dyn ModelRouter>,
        tools: Arc<dyn ToolDispatcher>,
        policy: Arc<dyn PolicyEngine>,
    ) -> Self {
        Self {
            model_router,
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

    async fn complete_model_turn(
        &self,
        run_id: Uuid,
        iteration: usize,
        agent: &str,
        principal_id: Option<String>,
        required: &[agent_models::ModelCapability],
        request: ModelRequest,
        cancellation: CancellationToken,
        deadline: Option<tokio::time::Instant>,
        events: &mut Vec<RunEvent>,
    ) -> Result<RoutedModelTurn, ModelTurnError> {
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let event_sink = Arc::new(ModelRoutingEventChannel(sender));
        let completion = self
            .model_router
            .complete(required, request, cancellation, event_sink);
        let result = match deadline {
            Some(deadline) => match tokio::time::timeout_at(deadline, completion).await {
                Ok(result) => Ok(result),
                Err(_) => Err(ModelTurnError::TimedOut),
            },
            None => Ok(completion.await),
        };

        let mut pending_attempts = Vec::new();
        while let Ok(event) = receiver.try_recv() {
            let kind = match event {
                ModelRoutingEvent::CandidatesDiscovered { model_ids } => {
                    RunEventKind::ModelCandidatesDiscovered { model_ids }
                }
                ModelRoutingEvent::CandidateAttemptStarted { model_id } => {
                    pending_attempts.push(model_id.clone());
                    RunEventKind::ModelCandidateAttemptStarted { model_id }
                }
                ModelRoutingEvent::CandidateAttemptFailed { model_id, category } => {
                    pending_attempts.retain(|candidate| candidate != &model_id);
                    let record = usage_record(
                        run_id,
                        iteration,
                        agent,
                        principal_id.clone(),
                        &model_id,
                        InvocationOutcome::Failed,
                        UsageDimensions::new(),
                    );
                    self.emit_event(
                        events,
                        RunEvent {
                            run_id,
                            iteration,
                            kind: RunEventKind::ModelUsageRecorded { record },
                        },
                    );
                    RunEventKind::ModelCandidateAttemptFailed { model_id, category }
                }
                ModelRoutingEvent::Fallback {
                    from_model_id,
                    to_model_id,
                    category,
                } => RunEventKind::ModelFallback {
                    from_model_id,
                    to_model_id,
                    category,
                },
                ModelRoutingEvent::ModelSelected { model_id } => {
                    RunEventKind::ModelSelected { model_id }
                }
                ModelRoutingEvent::RoutingFailed {
                    attempted_candidates,
                } => RunEventKind::ModelRoutingFailed {
                    attempted_candidates,
                },
            };
            self.emit_event(
                events,
                RunEvent {
                    run_id,
                    iteration,
                    kind,
                },
            );
        }

        match result {
            Ok(Ok(routed)) => {
                let mut record = usage_record(
                    run_id,
                    iteration,
                    agent,
                    principal_id,
                    &routed.model_id,
                    InvocationOutcome::Succeeded,
                    usage_dimensions(routed.turn.usage.as_ref()),
                );
                if let Some(provider_cost) = routed
                    .turn
                    .usage
                    .as_ref()
                    .and_then(|usage| usage.reported_cost.as_ref())
                {
                    if let Ok(cost) = ReportedCost::new(
                        provider_cost.amount_micros,
                        provider_cost.currency.clone(),
                    ) {
                        record = record.with_reported_cost(cost);
                    }
                }
                self.emit_event(
                    events,
                    RunEvent {
                        run_id,
                        iteration,
                        kind: RunEventKind::ModelUsageRecorded { record },
                    },
                );
                Ok(routed)
            }
            Ok(Err(error)) => {
                let outcome = if matches!(error, ModelError::Cancelled) {
                    InvocationOutcome::Cancelled
                } else {
                    InvocationOutcome::Failed
                };
                for model_id in pending_attempts {
                    let record = usage_record(
                        run_id,
                        iteration,
                        agent,
                        principal_id.clone(),
                        &model_id,
                        outcome,
                        UsageDimensions::new(),
                    );
                    self.emit_event(
                        events,
                        RunEvent {
                            run_id,
                            iteration,
                            kind: RunEventKind::ModelUsageRecorded { record },
                        },
                    );
                }
                Err(ModelTurnError::Model(error))
            }
            Err(error) => {
                for model_id in pending_attempts {
                    let record = usage_record(
                        run_id,
                        iteration,
                        agent,
                        principal_id.clone(),
                        &model_id,
                        InvocationOutcome::TimedOut,
                        UsageDimensions::new(),
                    );
                    self.emit_event(
                        events,
                        RunEvent {
                            run_id,
                            iteration,
                            kind: RunEventKind::ModelUsageRecorded { record },
                        },
                    );
                }
                Err(error)
            }
        }
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

    /// Runs an agent while forwarding each emitted runtime event to an additional sink.
    ///
    /// The runtime's configured sink remains active; the supplied sink is composed
    /// with it so existing observers continue to receive events.
    pub async fn run_with_event_sink(
        &self,
        run_id: Uuid,
        agent: &Agent,
        context: ToolContext,
        messages: Vec<Message>,
        input: impl Into<String>,
        cancellation: CancellationToken,
        event_sink: Arc<dyn EventSink>,
    ) -> Result<RunResult, RunFailure> {
        let runtime = Self {
            event_sink: Arc::new(CompositeEventSink {
                sinks: vec![Arc::clone(&self.event_sink), event_sink],
            }),
            ..self.clone()
        };
        runtime
            .run(run_id, agent, context, messages, input, cancellation)
            .await
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
        let timezone_context = context.timezone.as_deref().map(|timezone| {
            format!(
                "User timezone: {timezone}. This timezone comes from the authenticated user's profile and is authoritative. Use it when interpreting local dates, times, schedules, and relative time expressions; do not infer or override it from conversation content."
            )
        });
        // Give the model the internal recipient IDs for every family the host
        // has verified the user belongs to. Provider-specific Telegram IDs are
        // intentionally omitted by FamilyMemberContext serialization.
        let mut trusted_families = context.families.clone();
        if let Some(selected_family) = context.family.as_ref() {
            if !trusted_families
                .iter()
                .any(|family| family.family_id == selected_family.family_id)
            {
                trusted_families.push(selected_family.clone());
            }
        }
        let family_context = if trusted_families.is_empty() {
            None
        } else {
            let family_json = serde_json::to_string_pretty(&trusted_families)
                .expect("family contexts are serializable");
            Some(format!(
                "Trusted context for all families the authenticated user belongs to:\n{}\nUse these names, relationships, and recipient IDs as authoritative. Do not infer additional family members or relationships. If a name is unavailable, say so.",
                family_json,
            ))
        };
        let system_prompt = match (family_context.as_deref(), timezone_context.as_deref()) {
            (Some(family_context), Some(timezone_context)) => Some(format!(
                "{}\n\n{}\n\n{}\n\n{}",
                base_prompt, date_context, timezone_context, family_context
            )),
            (Some(family_context), None) => Some(format!(
                "{}\n\n{}\n\n{}",
                base_prompt, date_context, family_context
            )),
            (None, Some(timezone_context)) => Some(format!(
                "{}\n\n{}\n\n{}",
                base_prompt, date_context, timezone_context
            )),
            (None, None) => Some(format!("{}\n\n{}", base_prompt, date_context)),
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

            let routed = match self
                .complete_model_turn(
                    run_id,
                    iteration,
                    &agent.id,
                    context.user_id.clone(),
                    &agent.capabilities,
                    request,
                    cancellation.clone(),
                    deadline,
                    &mut events,
                )
                .await
            {
                Ok(routed) => routed,
                Err(ModelTurnError::TimedOut) => {
                    return Ok(self.stopped_result(
                        run_id,
                        iteration - 1,
                        StopReason::TimedOut,
                        messages,
                        &mut events,
                    ));
                }
                Err(ModelTurnError::Model(ModelError::Cancelled)) => {
                    return Ok(self.stopped_result(
                        run_id,
                        iteration - 1,
                        StopReason::Cancelled,
                        messages,
                        &mut events,
                    ));
                }
                Err(ModelTurnError::Model(error)) => {
                    let error = RuntimeError::Model(error);
                    self.emit_event(
                        &mut events,
                        RunEvent {
                            run_id,
                            iteration,
                            kind: RunEventKind::ModelFailed {
                                category: ErrorCategory::Model,
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
            };
            let model_id = routed.model_id;
            let turn = routed.turn;
            self.emit_event(
                &mut events,
                RunEvent {
                    run_id,
                    iteration,
                    kind: RunEventKind::ModelResponded {
                        model_id: model_id.clone(),
                        tool_calls: turn.tool_calls.len(),
                    },
                },
            );

            messages.push(Message::Assistant {
                content: turn.content.clone(),
                tool_calls: turn.tool_calls.clone(),
                model_id: Some(model_id),
                generated_at: Some(chrono::Utc::now()),
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
                usage: None,
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
            capabilities: Vec::new(),
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
            capabilities: Vec::new(),
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
                usage: None,
                content: Some("done".into()),
                tool_calls: Vec::new(),
            })
        }
    }

    #[tokio::test]
    async fn all_trusted_family_rosters_are_injected_into_model_system_prompt() {
        let model = Arc::new(CapturingModel(Mutex::new(None)));
        let runtime = AgentRuntime::new(
            model.clone(),
            Arc::new(ToolRegistry::new()),
            Arc::new(AllowListPolicy::new(Vec::<String>::new())),
        );
        let agent = Agent {
            id: "assistant".into(),
            capabilities: Vec::new(),
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
            families: vec![FamilyContext {
                family_id: "family-2".into(),
                family_name: "Other home".into(),
                family_role: "member".into(),
                relationship: "son".into(),
                members: vec![FamilyMemberContext {
                    user_id: "user-3".into(),
                    name: Some("Alex".into()),
                    birthday: None,
                    telegram_id: Some("987654321".into()),
                    role: "owner".into(),
                    relationship: "father".into(),
                }],
            }],
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
        assert!(prompt.contains("Other home"));
        assert!(prompt.contains("Alex"));
        assert!(prompt.contains("user-3"));
        assert!(prompt.contains("father"));
        assert!(prompt.contains("son"));
        assert!(prompt.contains("Do not infer additional family members"));
        assert!(prompt.contains("Current date and time:"));
        assert!(prompt.contains(&chrono::Utc::now().date_naive().to_string()));
        assert!(prompt.contains("recipient_id"));
        assert!(prompt.contains("user-1"));
        assert!(!prompt.contains("telegram_id"));
        assert!(!prompt.contains("987654321"));
    }
    #[tokio::test]
    async fn user_timezone_is_injected_as_authoritative_system_context() {
        let model = Arc::new(CapturingModel(Mutex::new(None)));
        let runtime = AgentRuntime::new(
            model.clone(),
            Arc::new(ToolRegistry::new()),
            Arc::new(AllowListPolicy::new(Vec::<String>::new())),
        );
        let agent = Agent {
            id: "assistant".into(),
            capabilities: Vec::new(),
            system_prompt: Some("You are a helpful assistant.".into()),
            tool_names: Vec::new(),
            max_iterations: 1,
        };

        runtime
            .run(
                Uuid::new_v4(),
                &agent,
                ToolContext {
                    timezone: Some("Europe/Madrid".into()),
                    ..ToolContext::default()
                },
                Vec::new(),
                "What time is it?",
                CancellationToken::new(),
            )
            .await
            .unwrap();

        let request = model.0.lock().unwrap().take().unwrap();
        let prompt = request.system_prompt.unwrap();
        assert!(prompt.contains("User timezone: Europe/Madrid"));
        assert!(
            prompt.contains("comes from the authenticated user's profile and is authoritative")
        );
    }
}
