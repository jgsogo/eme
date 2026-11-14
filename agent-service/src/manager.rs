use std::{collections::HashMap, sync::Arc};

use agent_runtime::{
    CancellationToken, RunEvent as RuntimeEvent, RunEventKind as RuntimeEventKind,
};
use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::{Mutex, mpsc};

use crate::{Run, RunEvent, RunEventKind, RunId, RunStatus};

/// Executes the work associated with an accepted run.
///
/// Implementations adapt the platform's agent runtime to this service contract.
#[async_trait]
pub trait RunExecutor: Send + Sync + 'static {
    async fn execute(
        &self,
        run_id: RunId,
        agent: String,
        input: String,
        cancellation: CancellationToken,
    ) -> Result<ExecutionOutcome, String>;

    async fn execute_with_events(
        &self,
        run_id: RunId,
        agent: String,
        input: String,
        cancellation: CancellationToken,
    ) -> Result<ExecutionReport, String> {
        self.execute(run_id, agent, input, cancellation)
            .await
            .map(|outcome| ExecutionReport {
                outcome,
                runtime_events: Vec::new(),
            })
    }
}

/// Outcome reported by a run executor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionOutcome {
    Completed(Option<String>),
    Cancelled,
    TimedOut,
    MaxIterationsReached,
    Failed(String),
}

/// Runtime outcome and the events emitted while executing it.
#[derive(Debug, Clone)]
pub struct ExecutionReport {
    pub outcome: ExecutionOutcome,
    pub runtime_events: Vec<RuntimeEvent>,
}

/// Configuration errors for a run manager.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RunManagerConfigError {
    #[error("max_concurrent_runs must be greater than zero")]
    ZeroConcurrency,
    #[error("max_queued_runs must be greater than zero")]
    ZeroQueueCapacity,
}

/// Errors returned when a run cannot be accepted.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SubmitRunError {
    #[error("the run queue is full")]
    QueueFull,
}

/// Result of requesting cancellation for a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelRunOutcome {
    /// Cancellation was signalled to a running execution.
    Requested,
    /// The queued run was cancelled before a worker started it.
    CancelledBeforeStart,
    /// A cancellation request had already been sent.
    AlreadyRequested,
    /// The run had already reached a terminal state.
    AlreadyFinished,
}

/// Errors returned when cancellation targets an unknown run.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CancelRunError {
    #[error("run not found")]
    NotFound,
}

struct ManagedRun {
    run: Run,
    cancellation: CancellationToken,
    events: Vec<RunEvent>,
}

impl ManagedRun {
    fn record_runtime(&mut self, event: &RuntimeEvent) {
        let (kind, tool_name, error_category) = match &event.kind {
            RuntimeEventKind::ModelRequested => (RunEventKind::ModelRequested, None, None),
            RuntimeEventKind::ModelResponded { .. } => (RunEventKind::ModelResponded, None, None),
            RuntimeEventKind::ToolExecutionStarted { name, .. } => {
                (RunEventKind::ToolExecutionStarted, Some(name.clone()), None)
            }
            RuntimeEventKind::ToolExecutionFinished {
                name,
                error_category,
                ..
            } => (
                RunEventKind::ToolExecutionFinished,
                Some(name.clone()),
                error_category.map(error_category_name),
            ),
            RuntimeEventKind::ModelFailed { category, .. } => (
                RunEventKind::ModelFailed,
                None,
                Some(error_category_name(*category)),
            ),
            RuntimeEventKind::Cancelled => (RunEventKind::RuntimeCancelled, None, None),
            RuntimeEventKind::TimedOut => (RunEventKind::RuntimeTimedOut, None, None),
        };
        self.events.push(RunEvent {
            sequence: self.events.len() as u64,
            kind,
            iteration: Some(event.iteration),
            tool_name,
            error_category,
            occurred_at: std::time::SystemTime::now(),
        });
    }

    /// Records a service or lifecycle event in the shared ordered history.
    fn record(&mut self, kind: RunEventKind) {
        self.events.push(RunEvent {
            sequence: self.events.len() as u64,
            kind,
            iteration: None,
            tool_name: None,
            error_category: None,
            occurred_at: std::time::SystemTime::now(),
        });
    }
}

fn error_category_name(category: agent_runtime::ErrorCategory) -> &'static str {
    match category {
        agent_runtime::ErrorCategory::Configuration => "configuration",
        agent_runtime::ErrorCategory::Authorization => "authorization",
        agent_runtime::ErrorCategory::Model => "model",
        agent_runtime::ErrorCategory::Tool => "tool",
        agent_runtime::ErrorCategory::Timeout => "timeout",
        agent_runtime::ErrorCategory::Cancellation => "cancellation",
    }
}

/// Coordinates run admission, in-memory state, and bounded execution.
#[derive(Clone)]
pub struct RunManager {
    runs: Arc<Mutex<HashMap<RunId, ManagedRun>>>,
    sender: mpsc::Sender<RunId>,
}

impl RunManager {
    /// Creates a manager with a bounded queue and a fixed number of workers.
    ///
    /// Must be called from within an active Tokio runtime.
    pub fn new(
        executor: Arc<dyn RunExecutor>,
        max_concurrent_runs: usize,
        max_queued_runs: usize,
    ) -> Result<Self, RunManagerConfigError> {
        if max_concurrent_runs == 0 {
            return Err(RunManagerConfigError::ZeroConcurrency);
        }
        if max_queued_runs == 0 {
            return Err(RunManagerConfigError::ZeroQueueCapacity);
        }

        let runs = Arc::new(Mutex::new(HashMap::<RunId, ManagedRun>::new()));
        let (sender, receiver) = mpsc::channel(max_queued_runs);
        let receiver = Arc::new(Mutex::new(receiver));

        for _ in 0..max_concurrent_runs {
            let runs = Arc::clone(&runs);
            let receiver = Arc::clone(&receiver);
            let executor = Arc::clone(&executor);

            tokio::spawn(async move {
                loop {
                    let run_id = {
                        let mut receiver = receiver.lock().await;
                        receiver.recv().await
                    };

                    let Some(run_id) = run_id else {
                        break;
                    };

                    let (run, cancellation) = {
                        let mut runs = runs.lock().await;
                        let Some(entry) = runs.get_mut(&run_id) else {
                            continue;
                        };

                        // Queued runs cancelled before dequeue are already terminal.
                        if entry.run.mark_running().is_err() {
                            continue;
                        }
                        entry.record(RunEventKind::Started);

                        (entry.run.clone(), entry.cancellation.clone())
                    };

                    let result = executor
                        .execute_with_events(
                            run.id,
                            run.agent.clone(),
                            run.input.clone(),
                            cancellation,
                        )
                        .await;

                    let mut runs = runs.lock().await;
                    if let Some(entry) = runs.get_mut(&run_id) {
                        // A run may have been cancelled while it was executing.
                        // Its executor reports Cancelled after observing the token.
                        if entry.run.status != RunStatus::Running {
                            continue;
                        }

                        if let Ok(report) = &result {
                            for runtime_event in &report.runtime_events {
                                entry.record_runtime(runtime_event);
                            }
                        }
                        let event = match result {
                            Ok(ExecutionReport {
                                outcome: ExecutionOutcome::Completed(response),
                                ..
                            }) => {
                                let _ = entry.run.complete(response);
                                RunEventKind::Completed
                            }
                            Ok(ExecutionReport {
                                outcome: ExecutionOutcome::Cancelled,
                                ..
                            }) => {
                                let _ = entry.run.cancel();
                                RunEventKind::Cancelled
                            }
                            Ok(ExecutionReport {
                                outcome: ExecutionOutcome::TimedOut,
                                ..
                            }) => {
                                let _ = entry.run.time_out();
                                RunEventKind::TimedOut
                            }
                            Ok(ExecutionReport {
                                outcome: ExecutionOutcome::MaxIterationsReached,
                                ..
                            }) => {
                                let _ = entry.run.reach_max_iterations();
                                RunEventKind::MaxIterationsReached
                            }
                            Ok(ExecutionReport {
                                outcome: ExecutionOutcome::Failed(error),
                                ..
                            }) => {
                                let _ = entry.run.fail(error);
                                RunEventKind::Failed
                            }
                            Err(error) => {
                                let _ = entry.run.fail(error);
                                RunEventKind::Failed
                            }
                        };
                        entry.record(event);
                    }
                }
            });
        }

        Ok(Self { runs, sender })
    }

    /// Accepts a run and returns its stable identifier once it is queued.
    ///
    /// If the bounded queue is full, the run is not retained by the manager.
    pub async fn submit(
        &self,
        agent: impl Into<String>,
        input: impl Into<String>,
    ) -> Result<RunId, SubmitRunError> {
        let run = Run::new(agent, input);
        let run_id = run.id;

        {
            let mut runs = self.runs.lock().await;
            runs.insert(
                run_id,
                ManagedRun {
                    run,
                    cancellation: CancellationToken::new(),
                    events: vec![RunEvent {
                        sequence: 0,
                        kind: RunEventKind::Queued,
                        iteration: None,
                        tool_name: None,
                        error_category: None,
                        occurred_at: std::time::SystemTime::now(),
                    }],
                },
            );
        }

        if self.sender.try_send(run_id).is_err() {
            self.runs.lock().await.remove(&run_id);
            return Err(SubmitRunError::QueueFull);
        }

        Ok(run_id)
    }

    /// Requests cancellation of a queued or running run.
    ///
    /// Queued runs transition to Cancelled immediately. Running executions
    /// receive a cancellation signal and stop at the runtime's next operation
    /// boundary. Repeated requests are safe and do not alter terminal runs.
    pub async fn cancel(&self, run_id: RunId) -> Result<CancelRunOutcome, CancelRunError> {
        let mut runs = self.runs.lock().await;
        let entry = runs.get_mut(&run_id).ok_or(CancelRunError::NotFound)?;

        match entry.run.status {
            RunStatus::Queued => {
                entry.cancellation.cancel();
                entry.run.cancel().expect("queued run can be cancelled");
                entry.record(RunEventKind::Cancelled);
                Ok(CancelRunOutcome::CancelledBeforeStart)
            }
            RunStatus::Running => {
                if entry.cancellation.is_cancelled() {
                    Ok(CancelRunOutcome::AlreadyRequested)
                } else {
                    entry.cancellation.cancel();
                    Ok(CancelRunOutcome::Requested)
                }
            }
            _ => Ok(CancelRunOutcome::AlreadyFinished),
        }
    }

    /// Returns an ordered snapshot of a run's lifecycle events.
    pub async fn events(&self, run_id: RunId) -> Option<Vec<RunEvent>> {
        self.runs
            .lock()
            .await
            .get(&run_id)
            .map(|entry| entry.events.clone())
    }

    /// Returns a snapshot of a run's current state.
    pub async fn get(&self, run_id: RunId) -> Option<Run> {
        self.runs
            .lock()
            .await
            .get(&run_id)
            .map(|entry| entry.run.clone())
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use agent_runtime::{
        CancellationToken, RunEvent as RuntimeEvent, RunEventKind as RuntimeEventKind,
    };
    use async_trait::async_trait;
    use tokio::sync::Notify;

    use super::{
        CancelRunError, CancelRunOutcome, ExecutionOutcome, RunExecutor, RunManager,
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
            _cancellation: CancellationToken,
        ) -> Result<ExecutionOutcome, String> {
            Ok(ExecutionOutcome::Completed(Some(format!(
                "processed: {input}"
            ))))
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
            _cancellation: CancellationToken,
        ) -> Result<ExecutionOutcome, String> {
            self.started.notify_one();
            self.release.notified().await;
            Ok(ExecutionOutcome::Completed(None))
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
            cancellation: CancellationToken,
        ) -> Result<ExecutionOutcome, String> {
            self.started.notify_one();
            while !cancellation.is_cancelled() {
                tokio::task::yield_now().await;
            }
            Ok(ExecutionOutcome::Cancelled)
        }
    }

    #[tokio::test]
    async fn submitted_run_is_executed_and_reaches_completed_state() {
        let manager = RunManager::new(Arc::new(ImmediateExecutor), 1, 4).unwrap();

        let run_id = manager.submit("assistant", "hello").await.unwrap();

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

        let first = manager.submit("assistant", "first").await.unwrap();
        started.notified().await;

        let _queued = manager.submit("assistant", "queued").await.unwrap();
        assert_eq!(
            manager.submit("assistant", "rejected").await,
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

        let _running = manager.submit("assistant", "running").await.unwrap();
        started.notified().await;
        let queued = manager.submit("assistant", "queued").await.unwrap();

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

        let run_id = manager.submit("assistant", "hello").await.unwrap();
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
            _run_id: RunId,
            _agent: String,
            _input: String,
            _cancellation: CancellationToken,
        ) -> Result<ExecutionOutcome, String> {
            Ok(ExecutionOutcome::Completed(None))
        }

        async fn execute_with_events(
            &self,
            run_id: RunId,
            agent: String,
            input: String,
            cancellation: CancellationToken,
        ) -> Result<super::ExecutionReport, String> {
            let outcome = self.execute(run_id, agent, input, cancellation).await?;
            Ok(super::ExecutionReport {
                outcome,
                runtime_events: vec![RuntimeEvent {
                    run_id: run_id.as_uuid(),
                    iteration: 2,
                    kind: RuntimeEventKind::ToolExecutionStarted {
                        tool_call_id: "call-1".into(),
                        name: "weather".into(),
                    },
                }],
            })
        }
    }

    #[tokio::test]
    async fn runtime_events_are_inserted_between_lifecycle_events() {
        let manager = RunManager::new(Arc::new(RuntimeEventExecutor), 1, 2).unwrap();
        let run_id = manager.submit("assistant", "hello").await.unwrap();

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
}
