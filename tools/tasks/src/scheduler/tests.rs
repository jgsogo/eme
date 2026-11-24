//! Tests for scheduled task execution.

use super::{next_occurrence, submission_retry_delay};
use crate::{NewTask, TaskRegistry, TaskScheduler};
use agent_runtime::{CancellationToken, ToolContext};
use agent_service::{ExecutionOutcome, ExecutionReport, FamilyService, RunExecutor, RunManager};
use agent_storage::{
    FileUserIdentityStore, InMemoryFamilyInvitationStore, InMemoryFamilyStore, UserIdentityStore,
};
use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, Timelike, Utc};
use cron::Schedule;
use std::{
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Notify, mpsc};
use uuid::Uuid;

struct RecordingExecutor {
    calls: mpsc::UnboundedSender<String>,
    block_first: bool,
    started: Arc<Notify>,
    release: Arc<Notify>,
    invocation: AtomicUsize,
}

#[async_trait]
impl RunExecutor for RecordingExecutor {
    async fn execute(
        &self,
        _run_id: agent_service::RunId,
        _agent: String,
        input: String,
        _history: Vec<agent_runtime::Message>,
        _context: ToolContext,
        _cancellation: CancellationToken,
    ) -> Result<ExecutionReport, String> {
        let invocation = self.invocation.fetch_add(1, Ordering::SeqCst);
        if self.block_first && invocation == 0 {
            self.started.notify_one();
            self.release.notified().await;
        }
        let _ = self.calls.send(input);
        Ok(ExecutionReport {
            outcome: ExecutionOutcome::Completed(None),
            runtime_events: Vec::new(),
            messages: None,
        })
    }
}

fn test_registry() -> (
    TaskRegistry,
    std::path::PathBuf,
    Arc<FileUserIdentityStore>,
    Arc<FamilyService>,
    String,
) {
    let directory = std::env::temp_dir().join(format!("task-scheduler-{}", Uuid::new_v4()));
    let registry = TaskRegistry::load(&directory).unwrap();
    registry.validate_agents(["assistant".to_owned()]).unwrap();
    let identities = Arc::new(FileUserIdentityStore::open(directory.join("users.json")).unwrap());
    let (user, _) = identities.resolve_or_create("telegram", "42", None).unwrap();
    let family_store = Arc::new(InMemoryFamilyStore::default());
    let invitations = Arc::new(InMemoryFamilyInvitationStore::new(Arc::clone(&family_store)));
    let families = Arc::new(FamilyService::new(family_store, invitations));
    (registry, directory, identities, families, user.id.to_string())
}

fn recording_manager(
    block_first: bool,
) -> (
    RunManager,
    mpsc::UnboundedReceiver<String>,
    Arc<Notify>,
    Arc<Notify>,
) {
    let (calls, receiver) = mpsc::unbounded_channel();
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let executor = RecordingExecutor {
        calls,
        block_first,
        started: Arc::clone(&started),
        release: Arc::clone(&release),
        invocation: AtomicUsize::new(0),
    };
    (
        RunManager::new(Arc::new(executor), 1, 1)
            .unwrap()
            .with_storage(Arc::new(agent_storage::InMemoryStorage::default())),
        receiver,
        started,
        release,
    )
}

fn new_task(schedule: Option<&str>, at: Option<chrono::DateTime<chrono::FixedOffset>>) -> NewTask {
    NewTask {
        name: "scheduled".into(),
        description: String::new(),
        agent: "assistant".into(),
        prompt: "scheduled prompt".into(),
        schedule: schedule.map(str::to_owned),
        at,
        timezone: schedule.map(|_| "UTC".into()),
        family_id: None,
    }
}

#[test]
fn calculates_recurring_occurrences_in_requested_timezone() {
    let schedule = Schedule::from_str("0 0 8 * * *").unwrap();
    let timezone = "Europe/Madrid".parse().unwrap();
    let after = Utc::now();
    let next = next_occurrence(&schedule, timezone, after).unwrap();
    assert!(next > after);
    assert_eq!(next.with_timezone(&timezone).hour(), 8);
}

#[test]
fn skips_missed_occurrences() {
    let schedule = Schedule::from_str("0 0 8 * * *").unwrap();
    let after = Utc::now() + ChronoDuration::days(2);
    assert!(next_occurrence(&schedule, "UTC".parse().unwrap(), after).unwrap() > after);
}

#[tokio::test]
async fn due_cron_task_is_submitted_to_the_run_manager() {
    let (registry, directory, identities, families, user_id) = test_registry();
    registry
        .add(&user_id, new_task(Some("*/1 * * * * *"), None))
        .unwrap();
    let (manager, mut calls, _, _) = recording_manager(false);
    let scheduler = TaskScheduler::start(registry.clone(), manager, identities, families);

    let input = tokio::time::timeout(Duration::from_secs(3), calls.recv())
        .await
        .expect("cron task should be submitted within one occurrence")
        .expect("executor should receive the scheduled task");
    assert_eq!(input, "scheduled prompt");

    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if registry.snapshot()[0]
                .next_run
                .is_some_and(|next| next > Utc::now())
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("next cron occurrence should be persisted after submission");

    scheduler.shutdown().await;
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn one_time_task_is_retained_and_retried_when_the_run_queue_is_full() {
    let (registry, directory, identities, families, user_id) = test_registry();
    let task = registry
        .add(
            &user_id,
            new_task(
                None,
                Some((Utc::now() + ChronoDuration::milliseconds(700)).fixed_offset()),
            ),
        )
        .unwrap();
    let (manager, mut calls, started, release) = recording_manager(true);

    manager
        .submit(
            "assistant",
            "occupy worker",
            Vec::new(),
            ToolContext::default(),
        )
        .await
        .unwrap();
    started.notified().await;
    manager
        .submit(
            "assistant",
            "fill queue",
            Vec::new(),
            ToolContext::default(),
        )
        .await
        .unwrap();

    let scheduler = TaskScheduler::start(registry.clone(), manager, identities, families);
    // Let the one-time task become due and encounter QueueFull before freeing capacity.
    tokio::time::sleep(Duration::from_millis(1_100)).await;
    release.notify_one();

    let received = tokio::time::timeout(Duration::from_secs(5), async {
        let mut received = Vec::new();
        while received.len() < 3 {
            if let Some(input) = calls.recv().await {
                received.push(input);
            }
        }
        received
    })
    .await
    .expect("scheduled task should be retried after queue capacity becomes available");

    assert_eq!(received[0], "occupy worker");
    assert_eq!(received[1], "fill queue");
    assert_eq!(received[2], "scheduled prompt");
    let completed = registry
        .snapshot()
        .into_iter()
        .find(|item| item.id == task.id)
        .expect("one-time task remains available for execution history");
    assert!(!completed.enabled);
    assert!(completed.next_run.is_none());

    scheduler.shutdown().await;
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn submission_retry_backoff_is_bounded() {
    assert_eq!(submission_retry_delay(1), Duration::from_secs(1));
    assert_eq!(submission_retry_delay(2), Duration::from_secs(5));
    assert_eq!(submission_retry_delay(3), Duration::from_secs(30));
    assert_eq!(submission_retry_delay(20), Duration::from_secs(30));
}

#[test]
fn recurring_cron_occurrences_respect_timezone_across_daylight_saving() {
    let schedule = Schedule::from_str("0 0 8 * * *").unwrap();
    let timezone = "Europe/Madrid".parse().unwrap();

    let before_spring = chrono::DateTime::parse_from_rfc3339("2026-03-28T07:01:00Z")
        .unwrap()
        .to_utc();
    let spring_next = next_occurrence(&schedule, timezone, before_spring).unwrap();
    assert_eq!(
        spring_next,
        chrono::DateTime::parse_from_rfc3339("2026-03-29T06:00:00Z")
            .unwrap()
            .to_utc()
    );

    let before_autumn = chrono::DateTime::parse_from_rfc3339("2026-10-24T06:01:00Z")
        .unwrap()
        .to_utc();
    let autumn_next = next_occurrence(&schedule, timezone, before_autumn).unwrap();
    assert_eq!(
        autumn_next,
        chrono::DateTime::parse_from_rfc3339("2026-10-25T07:00:00Z")
            .unwrap()
            .to_utc()
    );
}
