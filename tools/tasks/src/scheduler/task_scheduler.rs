//! Runs due persisted tasks and submits them to the run manager.

use super::ScheduledTask;
use crate::{Task, TaskRegistry};
use agent_runtime::ToolContext;
use agent_service::RunManager;
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use cron::Schedule;
use std::{collections::HashMap, time::Duration};
use tokio::{sync::watch, task::JoinHandle, time};

pub struct TaskScheduler {
    shutdown: watch::Sender<bool>,
    handle: Option<JoinHandle<()>>,
}

impl TaskScheduler {
    pub fn start(registry: TaskRegistry, manager: RunManager) -> Self {
        let (shutdown, receiver) = watch::channel(false);
        let changed = registry.subscribe();
        let initial = scheduled_tasks(registry.snapshot());
        let count = initial.len();
        let handle = tokio::spawn(run_scheduler(registry, manager, receiver, changed, initial));
        tracing::info!(tasks = count, "task scheduler started");
        Self {
            shutdown,
            handle: Some(handle),
        }
    }

    pub async fn shutdown(mut self) {
        let _ = self.shutdown.send(true);
        if let Some(handle) = self.handle.take()
            && let Err(error) = handle.await
        {
            tracing::warn!(%error, "task scheduler stopped unexpectedly");
        }
    }
}
impl Drop for TaskScheduler {
    fn drop(&mut self) {
        let _ = self.shutdown.send(true);
    }
}

fn scheduled_tasks(tasks: Vec<Task>) -> HashMap<uuid::Uuid, ScheduledTask> {
    tasks
        .into_iter()
        .filter_map(|task| {
            let next_run = task.next_run?;
            let schedule = task
                .schedule
                .as_deref()
                .map(str::parse::<Schedule>)
                .transpose()
                .ok()
                .flatten();
            let timezone = task
                .timezone
                .as_deref()
                .unwrap_or("UTC")
                .parse::<Tz>()
                .ok()?;
            Some((
                task.id,
                ScheduledTask {
                    id: task.id,
                    user_id: task.user_id,
                    name: task.name,
                    agent: task.agent,
                    prompt: task.prompt,
                    schedule,
                    timezone,
                    next_run,
                },
            ))
        })
        .collect()
}

async fn run_scheduler(
    registry: TaskRegistry,
    manager: RunManager,
    mut shutdown: watch::Receiver<bool>,
    mut changed: watch::Receiver<u64>,
    mut tasks: HashMap<uuid::Uuid, ScheduledTask>,
) {
    loop {
        if *shutdown.borrow() {
            break;
        }
        let next_run = tasks.values().map(|task| task.next_run).min();
        let delay = next_run.map(|at| (at - Utc::now()).to_std().unwrap_or(Duration::ZERO));
        tokio::select! {
            _ = async {
                if let Some(delay) = delay { time::sleep(delay).await; }
                else { std::future::pending::<()>().await; }
            } => {},
            result = changed.changed() => {
                if result.is_err() { break; }
                tasks = scheduled_tasks(registry.snapshot());
                tracing::debug!(tasks = tasks.len(), "scheduler refreshed task definitions");
                continue;
            }
            result = shutdown.changed() => {
                if result.is_err() || *shutdown.borrow() { break; }
            }
        }

        let now = Utc::now();
        let due = tasks
            .values()
            .filter(|task| task.next_run <= now)
            .map(|task| task.id)
            .collect::<Vec<_>>();
        for id in due {
            let Some(mut task) = tasks.remove(&id) else {
                continue;
            };
            let scheduled_at = task.next_run;
            let recurring = task.schedule.is_some();
            if let Some(schedule) = &task.schedule {
                if let Some(next) = next_occurrence(schedule, task.timezone, now) {
                    task.next_run = next;
                    registry.set_next_run(id, next);
                    tasks.insert(id, task.clone());
                } else {
                    tracing::warn!(task = %task.name, "recurring task has no further occurrence");
                }
            }
            match manager
                .submit(
                    task.agent.clone(),
                    task.prompt.clone(),
                    Vec::new(),
                    ToolContext::new(task.user_id.clone()),
                )
                .await
            {
                Ok(run_id) => {
                    tracing::info!(task = %task.name, user_id = %task.user_id, run_id = %run_id,
                    scheduled_at = %scheduled_at, "scheduled task submitted")
                }
                Err(error) => tracing::error!(task = %task.name, user_id = %task.user_id, %error,
                    "failed to submit scheduled task"),
            }
            if !recurring {
                registry.remove_after_execution(id);
            }
        }
    }
    tracing::info!("task scheduler stopped");
}

pub(super) fn next_occurrence(
    schedule: &Schedule,
    timezone: Tz,
    after: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    schedule
        .after(&after.with_timezone(&timezone))
        .next()
        .map(|date| date.with_timezone(&Utc))
}
