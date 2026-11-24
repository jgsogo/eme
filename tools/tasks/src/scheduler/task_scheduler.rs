//! Runs due persisted tasks and submits them to the run manager.

use super::ScheduledTask;
use crate::{Task, TaskRegistry};
use agent_runtime::ToolContext;
use agent_service::{FamilyService, RunManager};
use agent_storage::{FileUserIdentityStore, UserId, UserIdentityStore};
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use cron::Schedule;
use std::{collections::HashMap, str::FromStr, sync::Arc, time::Duration};
use tokio::{sync::watch, task::JoinHandle, time};

pub struct TaskScheduler {
    shutdown: watch::Sender<bool>,
    handle: Option<JoinHandle<()>>,
}

impl TaskScheduler {
    pub fn start(
        registry: TaskRegistry,
        manager: RunManager,
        identities: Arc<FileUserIdentityStore>,
        families: Arc<FamilyService>,
    ) -> Self {
        let (shutdown, receiver) = watch::channel(false);
        let changed = registry.subscribe();
        let initial = scheduled_tasks(registry.snapshot());
        let count = initial.len();
        let handle = tokio::spawn(run_scheduler(
            registry, manager, identities, families, receiver, changed, initial,
        ));
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

/// Resolve trusted execution metadata from the task owner and selected family.
/// Telegram identities and family membership are looked up at execution time.
async fn task_tool_context(
    task: &ScheduledTask,
    identities: &FileUserIdentityStore,
    family_service: &FamilyService,
) -> Result<ToolContext, String> {
    // New tasks are owned by stable EME user IDs. Resolve legacy transport-scoped
    // owners as well, so tasks created before identity migration remain executable.
    let telegram_identity = match UserId::from_str(&task.user_id) {
        Ok(user_id) => identities
            .find_identity_for_user("telegram", user_id)
            .map_err(|error| format!("resolving task owner's Telegram identity: {error}"))?,
        Err(_) => {
            let subject = task
                .user_id
                .strip_prefix("telegram:")
                .filter(|subject| !subject.is_empty())
                .ok_or_else(|| "task owner identifier is invalid".to_owned())?;
            identities
                .find_identity("telegram", subject)
                .map_err(|error| format!("resolving legacy task owner identity: {error}"))?
        }
    }
    .ok_or_else(|| "task owner has no linked Telegram identity".to_owned())?;
    let user_id = telegram_identity.user_id;
    let user = identities
        .get_user(user_id)
        .map_err(|error| format!("loading task owner profile: {error}"))?;

    let family = match task.family_id {
        Some(family_id) => {
            let mut context = family_service
                .context_for_member(user_id, family_id)
                .await
                .map_err(|error| format!("resolving selected family context: {error}"))?;
            for member in &mut context.members {
                let member_id = UserId::from_str(&member.user_id)
                    .map_err(|error| format!("invalid family member identifier: {error}"))?;
                member.telegram_id = identities
                    .find_identity_for_user("telegram", member_id)
                    .map_err(|error| format!("resolving family member Telegram identity: {error}"))?
                    .map(|identity| identity.subject);
                if let Some(profile) = identities
                    .get_user(member_id)
                    .map_err(|error| format!("loading family member profile: {error}"))?
                {
                    member.name = profile.display_name;
                    member.birthday = profile.birthday;
                }
            }
            Some(context)
        }
        None => None,
    };
    let families = family.iter().cloned().collect();

    Ok(ToolContext {
        user_id: Some(user_id.to_string()),
        timezone: user.and_then(|profile| profile.timezone),
        telegram_id: Some(telegram_identity.subject),
        family,
        families,
        ..ToolContext::default()
    })
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
                    family_id: task.family_id,
                    schedule,
                    timezone,
                    scheduled_for: next_run,
                    next_run,
                    retry_attempts: 0,
                },
            ))
        })
        .collect()
}

/// Runs at most one catch-up execution for each persisted due occurrence after restart.
/// Missed recurring ticks are coalesced; after a successful submission the next future
/// cron occurrence is calculated from the current time.
async fn run_scheduler(
    registry: TaskRegistry,
    manager: RunManager,
    identities: Arc<FileUserIdentityStore>,
    families: Arc<FamilyService>,
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
            let scheduled_at = task.scheduled_for;
            let submission = match task_tool_context(&task, &identities, &families).await {
                Ok(context) => manager
                    .submit_for_task(
                        task.id,
                        task.agent.clone(),
                        task.prompt.clone(),
                        Vec::new(),
                        context,
                    )
                    .await
                    .map_err(|error| error.to_string()),
                Err(error) => Err(error),
            };
            match submission {
                Ok(run_id) => {
                    tracing::info!(
                        task = %task.name,
                        user_id = %task.user_id,
                        run_id = %run_id,
                        scheduled_at = %scheduled_at,
                        "scheduled task submitted"
                    );
                    task.retry_attempts = 0;
                    if let Some(schedule) = &task.schedule {
                        if let Some(next) = next_occurrence(schedule, task.timezone, Utc::now()) {
                            task.scheduled_for = next;
                            task.next_run = next;
                            if let Err(error) = registry.set_next_run(id, next) {
                                // Keep the in-memory schedule advanced. The persisted due
                                // time remains pending so a restart retries (at-least-once).
                                tracing::error!(
                                    task = %task.name,
                                    %error,
                                    "failed to persist next scheduled occurrence"
                                );
                            }
                            tasks.insert(id, task);
                        } else {
                            tracing::warn!(
                                task = %task.name,
                                "recurring task has no further occurrence; disabling it"
                            );
                            if let Err(error) = registry.disable_after_execution(id) {
                                tracing::error!(task = %task.name, %error,
                                    "failed to disable exhausted scheduled task");
                            }
                        }
                    } else if let Err(error) = registry.disable_after_execution(id) {
                        tracing::error!(
                            task = %task.name,
                            %error,
                            "one-time task was submitted but could not be disabled"
                        );
                    }
                }
                Err(error) => {
                    task.retry_attempts = task.retry_attempts.saturating_add(1);
                    let delay = submission_retry_delay(task.retry_attempts);
                    task.next_run = Utc::now()
                        + chrono::Duration::from_std(delay)
                            .expect("retry delay fits in chrono duration");
                    tracing::error!(
                        task = %task.name,
                        user_id = %task.user_id,
                        %error,
                        attempt = task.retry_attempts,
                        retry_at = %task.next_run,
                        "failed to submit scheduled task; will retry"
                    );
                    // Keep scheduled_for unchanged: this exact occurrence remains pending.
                    tasks.insert(id, task);
                }
            }
        }
    }
    tracing::info!("task scheduler stopped");
}

pub(super) fn submission_retry_delay(attempt: u32) -> Duration {
    match attempt {
        0 | 1 => Duration::from_secs(1),
        2 => Duration::from_secs(5),
        _ => Duration::from_secs(30),
    }
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

#[cfg(test)]
mod context_tests {
    use super::*;
    use agent_storage::{
        FamilyRelationship, FileUserIdentityStore, InMemoryFamilyInvitationStore,
        InMemoryFamilyStore, UserIdentityStore,
    };
    use chrono::Utc;
    use std::{fs, sync::Arc};
    use uuid::Uuid;

    #[tokio::test]
    async fn resolves_legacy_owner_identity_and_selected_family_context() {
        let directory = std::env::temp_dir().join(format!("task-context-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let identities = FileUserIdentityStore::open(directory.join("users.json")).unwrap();
        let (user, _) = identities
            .resolve_or_create("telegram", "42", Some("Owner"))
            .unwrap();
        let family_store = Arc::new(InMemoryFamilyStore::default());
        let invitations = Arc::new(InMemoryFamilyInvitationStore::new(Arc::clone(
            &family_store,
        )));
        let family_service = FamilyService::new(family_store, invitations);
        let family = family_service
            .create_family(user.id, "Home", FamilyRelationship::Other)
            .await
            .unwrap();
        let now = Utc::now();
        let task = ScheduledTask {
            id: Uuid::new_v4(),
            user_id: "telegram:42".into(),
            name: "daily".into(),
            agent: "assistant".into(),
            prompt: "prepare report".into(),
            family_id: Some(family.id),
            schedule: None,
            timezone: "UTC".parse().unwrap(),
            scheduled_for: now,
            next_run: now,
            retry_attempts: 0,
        };

        let context = task_tool_context(&task, &identities, &family_service)
            .await
            .unwrap();
        let expected_user_id = user.id.to_string();
        let expected_family_id = family.id.to_string();
        assert_eq!(context.user_id.as_deref(), Some(expected_user_id.as_str()));
        assert_eq!(context.telegram_id.as_deref(), Some("42"));
        assert_eq!(
            context
                .family
                .as_ref()
                .map(|value| value.family_id.as_str()),
            Some(expected_family_id.as_str())
        );
        assert_eq!(context.families.len(), 1);
        assert_eq!(
            context.families[0].members[0].telegram_id.as_deref(),
            Some("42")
        );

        fs::remove_dir_all(directory).unwrap();
    }
}
