use std::{collections::HashMap, time::Duration};

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use cron::Schedule;
use tokio::{sync::watch, task::JoinHandle, time};

use crate::RunManager;

pub(crate) struct ScheduledTask {
    pub name: String,
    pub agent: String,
    pub prompt: String,
    pub schedule: Option<Schedule>,
    pub timezone: Tz,
    pub next_run: DateTime<Utc>,
}

/// Drives configured tasks and submits due work to the existing run manager.
pub struct TaskScheduler {
    shutdown: watch::Sender<bool>,
    handle: Option<JoinHandle<()>>,
}

impl TaskScheduler {
    /// Start the scheduler in the current Tokio runtime.
    pub fn start(registry: super::TaskRegistry, manager: RunManager) -> Self {
        let tasks = registry
            .into_tasks()
            .into_iter()
            .map(|task| (task.name.clone(), task))
            .collect::<HashMap<_, _>>();
        let task_count = tasks.len();
        let (shutdown, receiver) = watch::channel(false);
        let handle = tokio::spawn(run_scheduler(tasks, manager, receiver));

        tracing::info!(tasks = task_count, "task scheduler started");
        Self {
            shutdown,
            handle: Some(handle),
        }
    }

    /// Signal the scheduler to stop and wait for its loop to finish.
    pub async fn shutdown(mut self) {
        let _ = self.shutdown.send(true);
        if let Some(handle) = self.handle.take() {
            if let Err(error) = handle.await {
                tracing::warn!(%error, "task scheduler stopped unexpectedly");
            }
        }
    }
}

impl Drop for TaskScheduler {
    fn drop(&mut self) {
        let _ = self.shutdown.send(true);
    }
}

async fn run_scheduler(
    mut tasks: HashMap<String, ScheduledTask>,
    manager: RunManager,
    mut shutdown: watch::Receiver<bool>,
) {
    loop {
        if *shutdown.borrow() {
            break;
        }

        let Some(next_run) = tasks.values().map(|task| task.next_run).min() else {
            if shutdown.changed().await.is_err() || *shutdown.borrow() {
                break;
            }
            continue;
        };

        let delay = (next_run - Utc::now()).to_std().unwrap_or(Duration::ZERO);

        tokio::select! {
            _ = time::sleep(delay) => {}
            result = shutdown.changed() => {
                if result.is_err() || *shutdown.borrow() {
                    break;
                }
            }
        }

        let now = Utc::now();
        let due = tasks
            .iter()
            .filter(|(_, task)| task.next_run <= now)
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();

        for name in due {
            let Some(mut task) = tasks.remove(&name) else {
                continue;
            };
            let scheduled_at = task.next_run;
            let task_name = task.name.clone();
            let agent_name = task.agent.clone();

            if let Some(schedule) = &task.schedule {
                if let Some(next_run) = next_occurrence(schedule, task.timezone, now) {
                    task.next_run = next_run;
                    tasks.insert(name.clone(), task);
                } else {
                    tracing::warn!(task = %name, "recurring task has no further occurrence");
                }
            }

            match manager
                .submit(task.agent.clone(), task.prompt.clone())
                .await
            {
                Ok(run_id) => tracing::info!(
                    task = %task_name,
                    agent = %agent_name,
                    run_id = %run_id,
                    scheduled_at = %scheduled_at,
                    "scheduled task submitted"
                ),
                Err(error) => tracing::error!(
                    task = %task.name,
                    agent = %task.agent,
                    %error,
                    "failed to submit scheduled task"
                ),
            }
        }
    }

    tracing::info!("task scheduler stopped");
}

fn next_occurrence(
    schedule: &Schedule,
    timezone: Tz,
    after: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let local = after.with_timezone(&timezone);
    schedule
        .after(&local)
        .next()
        .map(|date| date.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Timelike, Utc};
    use cron::Schedule;
    use std::str::FromStr;

    use super::next_occurrence;

    #[test]
    fn calculates_recurring_occurrences_in_the_requested_timezone() {
        let schedule = Schedule::from_str("0 0 8 * * *").unwrap();
        let timezone = "Europe/Madrid".parse().unwrap();
        let after = Utc::now();

        let next = next_occurrence(&schedule, timezone, after).unwrap();
        assert!(next > after);
        assert_eq!(next.with_timezone(&timezone).hour(), 8);
    }

    #[test]
    fn skips_missed_occurrences_instead_of_replaying_them() {
        let schedule = Schedule::from_str("0 0 8 * * *").unwrap();
        let timezone = "UTC".parse().unwrap();
        let after = Utc::now() + Duration::days(2);

        let next = next_occurrence(&schedule, timezone, after).unwrap();
        assert!(next > after);
    }
}
