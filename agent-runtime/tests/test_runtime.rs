use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use tokio::sync::Notify;

use agent_runtime::{
    Agent, AgentModel, AgentRuntime, AllowListPolicy, CancellationToken, ErrorCategory, Message,
    ModelRequest, ModelTurn, RuntimeError, ToolCall, ToolDefinition, ToolDispatcher, ToolError,
};
use agent_models::error::ModelError;
use async_trait::async_trait;
use serde_json::{Value, json};

struct MockModel {
    responses: Mutex<VecDeque<ModelTurn>>,
    requests: Mutex<Vec<ModelRequest>>,
}

impl MockModel {
    fn new(responses: Vec<ModelTurn>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
            requests: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl AgentModel for MockModel {
    async fn complete(&self, request: ModelRequest) -> Result<ModelTurn, ModelError> {
        self.requests.lock().unwrap().push(request);

        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| ModelError::InvalidResponse("No more mock responses".into()))
    }
}

struct MockTools {
    calls: Mutex<Vec<(String, Value)>>,
}

impl MockTools {}

#[async_trait]
impl ToolDispatcher for MockTools {
    fn definition(&self, name: &str) -> Option<ToolDefinition> {
        match name {
            "calculator" => Some(ToolDefinition {
                name: "calculator".into(),
                description: "Multiplies two integers.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "a": { "type": "integer" },
                        "b": { "type": "integer" }
                    },
                    "required": ["a", "b"],
                    "additionalProperties": false
                }),
            }),
            _ => None,
        }
    }

    async fn execute(&self, name: &str, arguments: Value) -> Result<String, ToolError> {
        self.calls
            .lock()
            .unwrap()
            .push((name.to_string(), arguments.clone()));

        match name {
            "calculator" => {
                let a = arguments["a"].as_i64().unwrap();
                let b = arguments["b"].as_i64().unwrap();

                Ok((a * b).to_string())
            }
            _ => Err(ToolError::UnknownTool(name.into())),
        }
    }
}

#[tokio::test]
async fn executes_tool_and_returns_final_response() {
    let model = Arc::new(MockModel::new(vec![
        ModelTurn {
            content: None,
            tool_calls: vec![ToolCall {
                id: "call-1".into(),
                name: "calculator".into(),
                arguments: json!({
                    "a": 123,
                    "b": 456
                }),
                provider_metadata: None,
            }],
        },
        ModelTurn {
            content: Some("123 × 456 = 56088".into()),
            tool_calls: vec![],
        },
    ]));

    let tools = Arc::new(MockTools {
        calls: Mutex::new(Vec::new()),
    });

    let policy = Arc::new(AllowListPolicy::new(["calculator"]));

    let runtime = AgentRuntime::new(model.clone(), tools.clone(), policy);

    let agent = Agent {
        id: "calculator-agent".into(),
        system_prompt: Some("You are a helpful calculator.".into()),
        tool_names: vec!["calculator".into()],
        max_iterations: 5,
    };

    let result = runtime
        .run(
            &agent,
            "What is 123 multiplied by 456?",
            CancellationToken::new(),
        )
        .await
        .unwrap();

    assert_eq!(result.response.unwrap(), "123 × 456 = 56088");
    assert_eq!(result.iterations, 2);

    // The dispatcher executed exactly one tool call.
    let calls = tools.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "calculator");
    assert_eq!(calls[0].1, json!({ "a": 123, "b": 456 }));

    // The second model request contains the tool result.
    let requests = model.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);

    assert!(requests[1].messages.iter().any(|message| {
        matches!(
            message,
            Message::Tool {
                tool_call_id,
                content,
                ..
            } if tool_call_id == "call-1" && content == "56088"
        )
    }));
}

#[tokio::test]
async fn denies_unauthorized_tool_execution() {
    let model = Arc::new(MockModel::new(vec![
        ModelTurn {
            content: None,
            tool_calls: vec![ToolCall {
                id: "call-1".into(),
                name: "calculator".into(),
                arguments: json!({
                    "a": 123,
                    "b": 456
                }),
                provider_metadata: None,
            }],
        },
        ModelTurn {
            content: Some("I cannot use the calculator.".into()),
            tool_calls: vec![],
        },
    ]));

    let tools = Arc::new(MockTools {
        calls: Mutex::new(Vec::new()),
    });

    // Only "search" is allowed, not "calculator".
    let policy = Arc::new(AllowListPolicy::new(["search"]));

    let runtime = AgentRuntime::new(model.clone(), tools.clone(), policy);

    let agent = Agent {
        id: "restricted-agent".into(),
        system_prompt: None,
        tool_names: vec![],
        max_iterations: 5,
    };

    let result = runtime
        .run(
            &agent,
            "Calculate 123 multiplied by 456.",
            CancellationToken::new(),
        )
        .await
        .unwrap();

    assert_eq!(result.response.unwrap(), "I cannot use the calculator.");
    assert_eq!(result.iterations, 2);

    // The dispatcher must never receive the unauthorized call.
    assert!(tools.calls.lock().unwrap().is_empty());

    // The denial is returned to the model as a tool result.
    let requests = model.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);

    assert!(requests[1].messages.iter().any(|message| {
        matches!(
            message,
            Message::Tool {
                tool_call_id,
                content,
                ..
            } if tool_call_id == "call-1"
                && content.contains("Authorization denied")
        )
    }));
    assert!(result.events.iter().any(|event| {
        matches!(
            &event.kind,
            agent_runtime::RunEventKind::ToolExecutionFinished {
                error_category: Some(ErrorCategory::Authorization),
                ..
            }
        )
    }));
}

#[tokio::test]
async fn stops_after_max_iterations() {
    let model = Arc::new(MockModel::new(vec![
        ModelTurn {
            content: None,
            tool_calls: vec![ToolCall {
                id: "call-1".into(),
                name: "calculator".into(),
                arguments: json!({ "a": 2, "b": 3 }),
                provider_metadata: None,
            }],
        },
        ModelTurn {
            content: None,
            tool_calls: vec![ToolCall {
                id: "call-2".into(),
                name: "calculator".into(),
                arguments: json!({ "a": 4, "b": 5 }),
                provider_metadata: None,
            }],
        },
    ]));

    let tools = Arc::new(MockTools {
        calls: Mutex::new(Vec::new()),
    });

    let policy = Arc::new(AllowListPolicy::new(["calculator"]));

    let runtime = AgentRuntime::new(model, tools.clone(), policy);

    let agent = Agent {
        id: "calculator-agent".into(),
        system_prompt: None,
        tool_names: vec!["calculator".into()],
        max_iterations: 2,
    };

    let result = runtime
        .run(&agent, "Keep calculating.", CancellationToken::new())
        .await;

    let result = result.unwrap();

    assert_eq!(
        result.status,
        agent_runtime::RunStatus::MaxIterationsReached
    );
    assert_eq!(result.iterations, 2);
    assert!(result.response.is_none());

    // Two iterations were executed, each with one tool call.
    assert_eq!(tools.calls.lock().unwrap().len(), 2);
}

struct BlockingModel {
    started: Arc<Notify>,
    release: Arc<Notify>,
}

#[async_trait]
impl AgentModel for BlockingModel {
    async fn complete(&self, _request: ModelRequest) -> Result<ModelTurn, ModelError> {
        self.started.notify_one();
        self.release.notified().await;
        Ok(ModelTurn {
            content: Some("Model response".into()),
            tool_calls: Vec::new(),
        })
    }
}

#[tokio::test]
async fn cancellation_waits_for_in_flight_model_request_and_keeps_its_result() {
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let model = Arc::new(BlockingModel {
        started: started.clone(),
        release: release.clone(),
    });
    let runtime = Arc::new(AgentRuntime::new(
        model,
        Arc::new(MockTools {
            calls: Mutex::new(Vec::new()),
        }),
        Arc::new(AllowListPolicy::new(Vec::<String>::new())),
    ));
    let cancellation = CancellationToken::new();
    let run_cancellation = cancellation.clone();
    let agent = Agent {
        id: "test-agent".into(),
        system_prompt: None,
        tool_names: Vec::new(),
        max_iterations: 2,
    };

    let run = tokio::spawn(async move {
        runtime
            .run(&agent, "Hello", run_cancellation)
            .await
            .unwrap()
    });

    started.notified().await;
    cancellation.cancel();
    release.notify_one();

    let result = run.await.unwrap();
    assert_eq!(result.status, agent_runtime::RunStatus::Cancelled);
    assert_eq!(result.status.error_category(), Some(ErrorCategory::Cancellation));
    assert!(result.messages.iter().any(|message| {
        matches!(message, Message::Assistant { content: Some(content), .. } if content == "Model response")
    }));
    assert!(
        result
            .events
            .iter()
            .any(|event| { matches!(event.kind, agent_runtime::RunEventKind::Cancelled) })
    );
}

struct BlockingTools {
    started: Arc<Notify>,
    release: Arc<Notify>,
}

#[async_trait]
impl ToolDispatcher for BlockingTools {
    fn definition(&self, name: &str) -> Option<ToolDefinition> {
        (name == "calculator").then(|| ToolDefinition {
            name: "calculator".into(),
            description: "A blocking test tool".into(),
            parameters: json!({"type": "object"}),
        })
    }

    async fn execute(&self, _name: &str, _arguments: Value) -> Result<String, ToolError> {
        self.started.notify_one();
        self.release.notified().await;
        Ok("tool result".into())
    }
}

#[tokio::test]
async fn cancellation_waits_for_in_flight_tool_and_keeps_its_result() {
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let model = Arc::new(MockModel::new(vec![ModelTurn {
        content: None,
        tool_calls: vec![ToolCall {
            id: "call-1".into(),
            name: "calculator".into(),
            arguments: json!({}),
            provider_metadata: None,
        }],
    }]));
    let tools = Arc::new(BlockingTools {
        started: started.clone(),
        release: release.clone(),
    });
    let runtime = Arc::new(AgentRuntime::new(
        model,
        tools,
        Arc::new(AllowListPolicy::new(["calculator"])),
    ));
    let cancellation = CancellationToken::new();
    let run_cancellation = cancellation.clone();
    let agent = Agent {
        id: "test-agent".into(),
        system_prompt: None,
        tool_names: vec!["calculator".into()],
        max_iterations: 2,
    };

    let run = tokio::spawn(async move {
        runtime
            .run(&agent, "Use the tool", run_cancellation)
            .await
            .unwrap()
    });

    started.notified().await;
    cancellation.cancel();
    release.notify_one();

    let result = run.await.unwrap();
    assert_eq!(result.status, agent_runtime::RunStatus::Cancelled);
    assert_eq!(result.status.error_category(), Some(ErrorCategory::Cancellation));
    assert!(result.messages.iter().any(|message| {
        matches!(message, Message::Tool { tool_call_id, content, .. } if tool_call_id == "call-1" && content == "tool result")
    }));
}

struct LimitedTools {
    delay: Duration,
    output: String,
    completed: Arc<AtomicBool>,
}

#[async_trait]
impl ToolDispatcher for LimitedTools {
    fn definition(&self, name: &str) -> Option<ToolDefinition> {
        (name == "test").then(|| ToolDefinition {
            name: "test".into(),
            description: "A test tool".into(),
            parameters: json!({"type": "object"}),
        })
    }

    async fn execute(&self, _name: &str, _arguments: Value) -> Result<String, ToolError> {
        tokio::time::sleep(self.delay).await;
        self.completed.store(true, Ordering::SeqCst);
        Ok(self.output.clone())
    }
}

fn test_agent() -> Agent {
    Agent {
        id: "test-agent".into(),
        system_prompt: None,
        tool_names: vec!["test".into()],
        max_iterations: 2,
    }
}

fn test_tool_call() -> ToolCall {
    ToolCall {
        id: "call-1".into(),
        name: "test".into(),
        arguments: json!({}),
        provider_metadata: None,
    }
}

#[tokio::test]
async fn times_out_individual_tool_calls_without_timing_out_the_run() {
    let completed = Arc::new(AtomicBool::new(false));
    let model = Arc::new(MockModel::new(vec![
        ModelTurn {
            content: None,
            tool_calls: vec![test_tool_call()],
        },
        ModelTurn {
            content: Some("The tool timed out.".into()),
            tool_calls: Vec::new(),
        },
    ]));
    let runtime = AgentRuntime::new(
        model.clone(),
        Arc::new(LimitedTools {
            delay: Duration::from_millis(100),
            output: "late result".into(),
            completed: completed.clone(),
        }),
        Arc::new(AllowListPolicy::new(["test"])),
    )
    .with_timeout(Duration::from_secs(1))
    .with_tool_timeout(Duration::from_millis(10));

    let result = runtime
        .run(&test_agent(), "Run the tool", CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(result.status, agent_runtime::RunStatus::Completed);
    assert!(!completed.load(Ordering::SeqCst));
    assert!(result.messages.iter().any(|message| {
        matches!(message, Message::Tool { content, is_error: true, .. } if content.contains("timed out"))
    }));
    assert!(result.events.iter().any(|event| {
        matches!(
            &event.kind,
            agent_runtime::RunEventKind::ToolExecutionFinished {
                error_category: Some(ErrorCategory::Timeout),
                ..
            }
        )
    }));
    assert_eq!(model.requests.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn truncates_tool_output_to_the_configured_utf8_byte_limit() {
    let completed = Arc::new(AtomicBool::new(false));
    let model = Arc::new(MockModel::new(vec![
        ModelTurn {
            content: None,
            tool_calls: vec![test_tool_call()],
        },
        ModelTurn {
            content: Some("Done.".into()),
            tool_calls: Vec::new(),
        },
    ]));
    let runtime = AgentRuntime::new(
        model,
        Arc::new(LimitedTools {
            delay: Duration::ZERO,
            output: "é".repeat(100),
            completed: completed.clone(),
        }),
        Arc::new(AllowListPolicy::new(["test"])),
    )
    .with_max_tool_output_bytes(32);

    let result = runtime
        .run(&test_agent(), "Run the tool", CancellationToken::new())
        .await
        .unwrap();

    let content = result
        .messages
        .iter()
        .find_map(|message| match message {
            Message::Tool { content, .. } => Some(content),
            _ => None,
        })
        .unwrap();

    assert!(completed.load(Ordering::SeqCst));
    assert!(content.len() <= 32);
    assert!(content.ends_with("[tool output truncated]"));
}


#[tokio::test]
async fn model_failures_keep_their_category() {
    let runtime = AgentRuntime::new(
        Arc::new(MockModel::new(Vec::new())),
        Arc::new(MockTools {
            calls: Mutex::new(Vec::new()),
        }),
        Arc::new(AllowListPolicy::new(Vec::<String>::new())),
    );

    let failure = match runtime
        .run(&test_agent(), "Fail", CancellationToken::new())
        .await
    {
        Err(failure) => failure,
        Ok(_) => panic!("Expected a model failure"),
    };

    assert_eq!(failure.category(), ErrorCategory::Model);
    assert!(matches!(
        failure.error,
        RuntimeError::Model(ModelError::InvalidResponse(_))
    ));
    assert!(failure.events.iter().any(|event| {
        matches!(
            &event.kind,
            agent_runtime::RunEventKind::ModelFailed {
                category: ErrorCategory::Model,
                ..
            }
        )
    }));
}

#[test]
fn configuration_errors_have_a_stable_category() {
    let error = RuntimeError::from(agent_config::ConfigError::invalid(
        "agent",
        None::<std::path::PathBuf>,
        "invalid definition",
    ));

    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(
        ToolError::Configuration("missing setting".into()).category(),
        ErrorCategory::Configuration
    );
}
