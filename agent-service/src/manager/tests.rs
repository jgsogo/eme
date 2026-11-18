//! Tests for run manager admission, execution, and cancellation.

use std::{sync::Arc, time::Duration};

use agent_runtime::{
    CancellationToken, FamilyContext, ModelFailureCategory, RunEvent as RuntimeEvent,
    RunEventKind as RuntimeEventKind, ToolContext,
};
use async_trait::async_trait;
use tokio::sync::Notify;

use super::{
    CancelRunError, CancelRunOutcome, ExecutionOutcome, ExecutionReport, RunExecutor, RunManager,
    RunManagerConfigError, SubmitRunError,
};
use crate::{RunEventKind, RunId, RunStatus};

struct ImmediateExecutor;

#[async_trait]
impl RunExecutor for ImmediateExecutor {
    async fn execute(
        &self,
        _run_id: RunId,
        _agent: String,
        input: String,
        _history: Vec<agent_runtime::Message>,
        _context: ToolContext,
        _cancellation: CancellationToken,
    ) -> Result<ExecutionReport, String> {
        Ok(ExecutionReport {
            outcome: ExecutionOutcome::Completed(Some(format!("processed: {input}"))),
            runtime_events: Vec::new(),
            messages: None,
        })
    }
}

struct BlockingExecutor {
    started: Arc<Notify>,
    release: Arc<Notify>,
}

#[async_trait]
impl RunExecutor for BlockingExecutor {
    async fn execute(
        &self,
        _run_id: RunId,
        _agent: String,
        _input: String,
        _history: Vec<agent_runtime::Message>,
        _context: ToolContext,
        _cancellation: CancellationToken,
    ) -> Result<ExecutionReport, String> {
        self.started.notify_one();
        self.release.notified().await;
        Ok(ExecutionReport {
            outcome: ExecutionOutcome::Completed(None),
            runtime_events: Vec::new(),
            messages: None,
        })
    }
}

struct CancellationAwareExecutor {
    started: Arc<Notify>,
}

#[async_trait]
impl RunExecutor for CancellationAwareExecutor {
    async fn execute(
        &self,
        _run_id: RunId,
        _agent: String,
        _input: String,
        _history: Vec<agent_runtime::Message>,
        _context: ToolContext,
        cancellation: CancellationToken,
    ) -> Result<ExecutionReport, String> {
        self.started.notify_one();
        while !cancellation.is_cancelled() {
            tokio::task::yield_now().await;
        }
        Ok(ExecutionReport {
            outcome: ExecutionOutcome::Cancelled,
            runtime_events: Vec::new(),
            messages: None,
        })
    }
}

struct ContextCaptureExecutor {
    received: Arc<std::sync::Mutex<Option<ToolContext>>>,
}

#[async_trait]
impl RunExecutor for ContextCaptureExecutor {
    async fn execute(
        &self,
        _run_id: RunId,
        _agent: String,
        _input: String,
        _history: Vec<agent_runtime::Message>,
        context: ToolContext,
        _cancellation: CancellationToken,
    ) -> Result<ExecutionReport, String> {
        *self.received.lock().unwrap() = Some(context);
        Ok(ExecutionReport {
            outcome: ExecutionOutcome::Completed(None),
            runtime_events: Vec::new(),
            messages: None,
        })
    }
}

#[tokio::test]
async fn trusted_context_survives_queue_and_reaches_executor() {
    let received = Arc::new(std::sync::Mutex::new(None));
    let manager = RunManager::new(
        Arc::new(ContextCaptureExecutor {
            received: received.clone(),
        }),
        1,
        2,
    )
    .unwrap();
    let context = ToolContext {
        user_id: Some("user-42".into()),
        telegram_id: None,
        family: Some(FamilyContext {
            family_id: "family-1".into(),
            family_name: "Home".into(),
            family_role: "owner".into(),
            relationship: "father".into(),
            members: Vec::new(),
        }),
        conversation_id: Some("conversation-1".into()),
        run_id: None,
    };

    let run_id = manager
        .submit("assistant", "hello", Vec::new(), context)
        .await
        .unwrap();

    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if manager
                .get(run_id)
                .await
                .is_some_and(|run| run.status == RunStatus::Completed)
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("run should complete");

    let received = received.lock().unwrap().clone().unwrap();
    assert_eq!(received.user_id.as_deref(), Some("user-42"));
    assert_eq!(received.conversation_id.as_deref(), Some("conversation-1"));
    assert_eq!(
        received
            .family
            .as_ref()
            .map(|family| family.family_id.as_str()),
        Some("family-1")
    );
    assert_eq!(
        received
            .family
            .as_ref()
            .map(|family| family.family_role.as_str()),
        Some("owner")
    );
    assert_eq!(received.run_id, Some(run_id.as_uuid()));
}

#[tokio::test]
async fn submitted_run_is_executed_and_reaches_completed_state() {
    let manager = RunManager::new(Arc::new(ImmediateExecutor), 1, 4).unwrap();

    let run_id = manager
        .submit("assistant", "hello", Vec::new(), ToolContext::default())
        .await
        .unwrap();

    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if let Some(run) = manager.get(run_id).await {
                if run.status == RunStatus::Completed {
                    assert_eq!(run.response.as_deref(), Some("processed: hello"));
                    break;
                }
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("run should complete");
}

#[tokio::test]
async fn queue_capacity_is_enforced() {
    let executor = BlockingExecutor {
        started: Arc::new(Notify::new()),
        release: Arc::new(Notify::new()),
    };
    let started = Arc::clone(&executor.started);
    let release = Arc::clone(&executor.release);
    let manager = RunManager::new(Arc::new(executor), 1, 1).unwrap();

    let first = manager
        .submit("assistant", "first", Vec::new(), ToolContext::default())
        .await
        .unwrap();
    started.notified().await;

    let _queued = manager
        .submit("assistant", "queued", Vec::new(), ToolContext::default())
        .await
        .unwrap();
    assert_eq!(
        manager
            .submit("assistant", "rejected", Vec::new(), ToolContext::default())
            .await,
        Err(SubmitRunError::QueueFull)
    );

    release.notify_one();

    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if manager
                .get(first)
                .await
                .is_some_and(|run| run.status == RunStatus::Completed)
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("first run should complete");
}

#[tokio::test]
async fn queued_run_can_be_cancelled_before_a_worker_starts_it() {
    let executor = BlockingExecutor {
        started: Arc::new(Notify::new()),
        release: Arc::new(Notify::new()),
    };
    let started = Arc::clone(&executor.started);
    let release = Arc::clone(&executor.release);
    let manager = RunManager::new(Arc::new(executor), 1, 2).unwrap();

    let _running = manager
        .submit("assistant", "running", Vec::new(), ToolContext::default())
        .await
        .unwrap();
    started.notified().await;
    let queued = manager
        .submit("assistant", "queued", Vec::new(), ToolContext::default())
        .await
        .unwrap();

    assert_eq!(
        manager.cancel(queued).await,
        Ok(CancelRunOutcome::CancelledBeforeStart)
    );
    assert_eq!(
        manager.get(queued).await.unwrap().status,
        RunStatus::Cancelled
    );
    assert_eq!(
        manager.cancel(queued).await,
        Ok(CancelRunOutcome::AlreadyFinished)
    );

    release.notify_one();
}

#[tokio::test]
async fn running_run_receives_cancellation_and_becomes_cancelled() {
    let executor = CancellationAwareExecutor {
        started: Arc::new(Notify::new()),
    };
    let started = Arc::clone(&executor.started);
    let manager = RunManager::new(Arc::new(executor), 1, 2).unwrap();

    let run_id = manager
        .submit("assistant", "hello", Vec::new(), ToolContext::default())
        .await
        .unwrap();
    started.notified().await;

    assert_eq!(
        manager.cancel(run_id).await,
        Ok(CancelRunOutcome::Requested)
    );
    assert_eq!(
        manager.cancel(run_id).await,
        Ok(CancelRunOutcome::AlreadyRequested)
    );

    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if manager
                .get(run_id)
                .await
                .is_some_and(|run| run.status == RunStatus::Cancelled)
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("running run should observe cancellation");
}

struct RuntimeEventExecutor;

#[async_trait]
impl RunExecutor for RuntimeEventExecutor {
    async fn execute(
        &self,
        run_id: RunId,
        _agent: String,
        _input: String,
        _history: Vec<agent_runtime::Message>,
        _context: ToolContext,
        _cancellation: CancellationToken,
    ) -> Result<ExecutionReport, String> {
        Ok(ExecutionReport {
            outcome: ExecutionOutcome::Completed(None),
            runtime_events: vec![RuntimeEvent {
                run_id: run_id.as_uuid(),
                iteration: 2,
                kind: RuntimeEventKind::ToolExecutionStarted {
                    tool_call_id: "call-1".into(),
                    name: "weather".into(),
                },
            }],
            messages: None,
        })
    }
}

#[tokio::test]
async fn runtime_events_are_inserted_between_lifecycle_events() {
    let manager = RunManager::new(Arc::new(RuntimeEventExecutor), 1, 2).unwrap();
    let run_id = manager
        .submit("assistant", "hello", Vec::new(), ToolContext::default())
        .await
        .unwrap();

    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let events = manager.events(run_id).await.unwrap();
            if events
                .last()
                .is_some_and(|event| event.kind == RunEventKind::Completed)
            {
                assert_eq!(
                    events
                        .iter()
                        .map(|event| event.sequence)
                        .collect::<Vec<_>>(),
                    vec![0, 1, 2, 3]
                );
                assert_eq!(events[2].kind, RunEventKind::ToolExecutionStarted);
                assert_eq!(events[2].iteration, Some(2));
                assert_eq!(events[2].tool_name.as_deref(), Some("weather"));
                assert_eq!(events[2].error_category, None);
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("runtime events should be ordered with lifecycle events");
}

struct ModelRoutingEventExecutor;

#[async_trait]
impl RunExecutor for ModelRoutingEventExecutor {
    async fn execute(
        &self,
        run_id: RunId,
        _agent: String,
        _input: String,
        _history: Vec<agent_runtime::Message>,
        _context: ToolContext,
        _cancellation: CancellationToken,
    ) -> Result<ExecutionReport, String> {
        Ok(ExecutionReport {
            outcome: ExecutionOutcome::Completed(None),
            runtime_events: vec![
                RuntimeEvent {
                    run_id: run_id.as_uuid(),
                    iteration: 1,
                    kind: RuntimeEventKind::ModelCandidatesDiscovered {
                        model_ids: vec!["provider/first".into(), "provider/second".into()],
                    },
                },
                RuntimeEvent {
                    run_id: run_id.as_uuid(),
                    iteration: 1,
                    kind: RuntimeEventKind::ModelCandidateAttemptStarted {
                        model_id: "provider/first".into(),
                    },
                },
                RuntimeEvent {
                    run_id: run_id.as_uuid(),
                    iteration: 1,
                    kind: RuntimeEventKind::ModelCandidateAttemptFailed {
                        model_id: "provider/first".into(),
                        category: ModelFailureCategory::RateLimited,
                    },
                },
                RuntimeEvent {
                    run_id: run_id.as_uuid(),
                    iteration: 1,
                    kind: RuntimeEventKind::ModelFallback {
                        from_model_id: "provider/first".into(),
                        to_model_id: "provider/second".into(),
                        category: ModelFailureCategory::RateLimited,
                    },
                },
                RuntimeEvent {
                    run_id: run_id.as_uuid(),
                    iteration: 1,
                    kind: RuntimeEventKind::ModelSelected {
                        model_id: "provider/second".into(),
                    },
                },
                RuntimeEvent {
                    run_id: run_id.as_uuid(),
                    iteration: 1,
                    kind: RuntimeEventKind::ModelResponded {
                        model_id: "provider/second".into(),
                        tool_calls: 0,
                    },
                },
            ],
            messages: None,
        })
    }
}

#[tokio::test]
async fn model_routing_events_preserve_candidates_failures_and_fallback_metadata() {
    let manager = RunManager::new(Arc::new(ModelRoutingEventExecutor), 1, 2).unwrap();
    let run_id = manager
        .submit("assistant", "hello", Vec::new(), ToolContext::default())
        .await
        .unwrap();

    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let events = manager.events(run_id).await.unwrap();
            if events
                .last()
                .is_some_and(|event| event.kind == RunEventKind::Completed)
            {
                let candidates = events
                    .iter()
                    .find(|event| event.kind == RunEventKind::ModelCandidatesDiscovered)
                    .unwrap();
                assert_eq!(
                    candidates.model_candidates.as_deref(),
                    Some(&["provider/first".to_owned(), "provider/second".to_owned()][..])
                );

                let failed = events
                    .iter()
                    .find(|event| event.kind == RunEventKind::ModelCandidateAttemptFailed)
                    .unwrap();
                assert_eq!(failed.model_id.as_deref(), Some("provider/first"));
                assert_eq!(failed.error_category, Some("rate_limited"));

                let fallback = events
                    .iter()
                    .find(|event| event.kind == RunEventKind::ModelFallback)
                    .unwrap();
                assert_eq!(fallback.model_id.as_deref(), Some("provider/first"));
                assert_eq!(fallback.next_model_id.as_deref(), Some("provider/second"));
                assert_eq!(fallback.error_category, Some("rate_limited"));

                let selected = events
                    .iter()
                    .find(|event| event.kind == RunEventKind::ModelSelected)
                    .unwrap();
                assert_eq!(selected.model_id.as_deref(), Some("provider/second"));
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("model routing events should be recorded");
}

#[tokio::test]
async fn cancellation_of_unknown_run_returns_not_found() {
    let manager = RunManager::new(Arc::new(ImmediateExecutor), 1, 2).unwrap();

    assert_eq!(
        manager.cancel(RunId::new()).await,
        Err(CancelRunError::NotFound)
    );
}

#[test]
fn rejects_zero_worker_or_queue_capacity() {
    assert_eq!(
        RunManager::new(Arc::new(ImmediateExecutor), 0, 1).err(),
        Some(RunManagerConfigError::ZeroConcurrency)
    );
    assert_eq!(
        RunManager::new(Arc::new(ImmediateExecutor), 1, 0).err(),
        Some(RunManagerConfigError::ZeroQueueCapacity)
    );
}
