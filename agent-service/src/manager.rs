use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::{Mutex, mpsc};

use crate::{Run, RunId};

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
    ) -> Result<Option<String>, String>;
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

/// Coordinates run admission, in-memory state, and bounded execution.
#[derive(Clone)]
pub struct RunManager {
    runs: Arc<Mutex<HashMap<RunId, Run>>>,
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

        let runs = Arc::new(Mutex::new(HashMap::<RunId, Run>::new()));
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

                    let run = {
                        let mut runs = runs.lock().await;
                        let Some(run) = runs.get_mut(&run_id) else {
                            continue;
                        };

                        if run.mark_running().is_err() {
                            continue;
                        }

                        run.clone()
                    };

                    let result = executor
                        .execute(run.id, run.agent.clone(), run.input.clone())
                        .await;

                    let mut runs = runs.lock().await;
                    if let Some(run) = runs.get_mut(&run_id) {
                        match result {
                            Ok(response) => {
                                let _ = run.complete(response);
                            }
                            Err(error) => {
                                let _ = run.fail(error);
                            }
                        }
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
            runs.insert(run_id, run);
        }

        if self.sender.try_send(run_id).is_err() {
            self.runs.lock().await.remove(&run_id);
            return Err(SubmitRunError::QueueFull);
        }

        Ok(run_id)
    }

    /// Returns a snapshot of a run's current state.
    pub async fn get(&self, run_id: RunId) -> Option<Run> {
        self.runs.lock().await.get(&run_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use async_trait::async_trait;
    use tokio::sync::Notify;

    use super::{RunExecutor, RunManager, RunManagerConfigError, SubmitRunError};
    use crate::{RunId, RunStatus};

    struct ImmediateExecutor;

    #[async_trait]
    impl RunExecutor for ImmediateExecutor {
        async fn execute(
            &self,
            _run_id: RunId,
            _agent: String,
            input: String,
        ) -> Result<Option<String>, String> {
            Ok(Some(format!("processed: {input}")))
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
        ) -> Result<Option<String>, String> {
            self.started.notify_one();
            self.release.notified().await;
            Ok(None)
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
