//! Coordinates run admission, bounded execution, cancellation, and supervision.

use super::{
    CancelRunError, CancelRunOutcome, ExecutionOutcome, ExecutionReport, ManagedRun, RunExecutor,
    RunManagerConfigError, SubmitRunError,
};
use crate::{Run, RunEvent, RunEventKind, RunId, RunStatus};
use agent_runtime::{CancellationToken, Message, ToolContext};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{Mutex, mpsc};
use tracing::info;

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

                    let (run, cancellation, history, context) = {
                        let mut runs = runs.lock().await;
                        let Some(entry) = runs.get_mut(&run_id) else {
                            continue;
                        };

                        // Queued runs cancelled before dequeue are already terminal.
                        if entry.run.mark_running().is_err() {
                            continue;
                        }
                        entry.record(RunEventKind::Started);

                        (
                            entry.run.clone(),
                            entry.cancellation.clone(),
                            entry.history.clone(),
                            entry.context.clone(),
                        )
                    };

                    info!(run_id = %run.id, agent = %run.agent, "agent run started");

                    let result = executor
                        .execute(
                            run.id,
                            run.agent.clone(),
                            run.input.clone(),
                            history,
                            context,
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
                            if let Some(messages) = &report.messages {
                                entry.history = messages.clone();
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
                        info!(run_id = %run_id, agent = %entry.run.agent, status = ?entry.run.status, "agent run finished");
                    }
                }
            });
        }

        Ok(Self { runs, sender })
    }

    /// Queues an agent run with its conversation history and trusted execution context.
    pub async fn submit(
        &self,
        agent: impl Into<String>,
        input: impl Into<String>,
        history: Vec<Message>,
        mut context: ToolContext,
    ) -> Result<RunId, SubmitRunError> {
        let run = Run::new(agent, input);
        let run_id = run.id;
        context.run_id = Some(run_id.as_uuid());

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
                    history,
                    context: context.clone(),
                },
            );
        }

        if self.sender.try_send(run_id).is_err() {
            self.runs.lock().await.remove(&run_id);
            return Err(SubmitRunError::QueueFull);
        }

        info!(run_id = %run_id, "agent conversation run queued");
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

    /// Returns a snapshot of all known runs, newest first by creation time.
    pub async fn list(&self) -> Vec<Run> {
        let mut runs: Vec<Run> = self
            .runs
            .lock()
            .await
            .values()
            .map(|entry| entry.run.clone())
            .collect();
        runs.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        runs
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

    /// Returns the latest complete conversation history known for a run.
    pub async fn history(&self, run_id: RunId) -> Option<Vec<Message>> {
        self.runs
            .lock()
            .await
            .get(&run_id)
            .map(|entry| entry.history.clone())
    }
}
