use super::*;
use crate::RunId;
use agent_runtime::{
    CancellationToken, EventSink, Message, RunEvent as RuntimeEvent,
    RunEventKind as RuntimeEventKind, ToolContext,
};
use agent_storage::{
    ConversationMessageStore, RunEventStore, RunStore, SqliteConversationStore,
    SqliteExecutionStore, StoredRun, StoredRunEvent, StoredRunStatus,
};
use async_trait::async_trait;
use std::{sync::Arc, time::Duration};
use tokio::sync::Notify;
use uuid::Uuid;

struct TranscriptExecutor;

#[async_trait]
impl RunExecutor for TranscriptExecutor {
    async fn execute(
        &self,
        _run_id: RunId,
        _agent: String,
        input: String,
        mut history: Vec<Message>,
        _context: ToolContext,
        _cancellation: CancellationToken,
    ) -> Result<ExecutionReport, String> {
        history.push(Message::User { content: input });
        history.push(Message::Assistant {
            content: Some("persisted answer".into()),
            tool_calls: Vec::new(),
            model_id: Some("test/model".into()),
            generated_at: Some(chrono::Utc::now()),
        });
        Ok(ExecutionReport {
            outcome: ExecutionOutcome::Completed(Some("persisted answer".into())),
            runtime_events: Vec::new(),
            messages: Some(history),
        })
    }
}

#[tokio::test]
async fn run_and_conversation_transcript_survive_manager_restart() {
    let directory = std::env::temp_dir().join(format!("eme-run-resume-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let database = directory.join("conversations.sqlite");
    let conversations = Arc::new(SqliteConversationStore::open(&database).unwrap());
    let conversation = conversations.create("owner", "assistant").unwrap();
    let execution = Arc::new(SqliteExecutionStore::open(&database).unwrap());

    let manager = RunManager::new(Arc::new(TranscriptExecutor), 1, 2)
        .unwrap()
        .with_storage(execution.clone());
    let run_id = manager
        .submit(
            "assistant",
            "current prompt",
            Vec::new(),
            ToolContext {
                conversation_id: Some(conversation.id.to_string()),
                ..ToolContext::default()
            },
        )
        .await
        .unwrap();

    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let run = manager.get(run_id).await.unwrap();
            let messages = execution
                .list_messages(Uuid::parse_str(&conversation.id.to_string()).unwrap())
                .await
                .unwrap();
            if run.status == crate::RunStatus::Completed && messages.len() == 2 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("run and transcript should be persisted");

    let run = execution.get_run(run_id.as_uuid()).await.unwrap().unwrap();
    assert_eq!(run.status, agent_storage::StoredRunStatus::Completed);
    let events = execution.list_events(run_id.as_uuid(), None).await.unwrap();
    assert_eq!(
        events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert_eq!(events[0].kind, "queued");
    assert_eq!(events[1].kind, "started");
    assert_eq!(events[2].kind, "completed");

    let conversation_id = Uuid::parse_str(&conversation.id.to_string()).unwrap();
    let stored_messages = execution.list_messages(conversation_id).await.unwrap();
    assert!(
        matches!(&stored_messages[0].message, Message::User { content } if content == "current prompt")
    );
    assert!(
        matches!(&stored_messages[1].message, Message::Assistant { content: Some(content), .. } if content == "persisted answer")
    );
    assert!(
        stored_messages
            .iter()
            .all(|message| message.run_id == Some(run_id.as_uuid()))
    );

    drop(manager);
    drop(execution);
    drop(conversations);
    let reopened = SqliteExecutionStore::open(&database).unwrap();
    assert_eq!(
        reopened
            .get_run(run_id.as_uuid())
            .await
            .unwrap()
            .unwrap()
            .status,
        agent_storage::StoredRunStatus::Completed
    );
    assert_eq!(
        reopened.list_messages(conversation_id).await.unwrap().len(),
        2
    );
    drop(reopened);
    let _ = std::fs::remove_dir_all(directory);
}

struct LiveEventExecutor {
    started: Arc<Notify>,
    release: Arc<Notify>,
}

#[async_trait]
impl RunExecutor for LiveEventExecutor {
    async fn execute(
        &self,
        _run_id: RunId,
        _agent: String,
        _input: String,
        _history: Vec<Message>,
        _context: ToolContext,
        _cancellation: CancellationToken,
    ) -> Result<ExecutionReport, String> {
        Err("the live event executor must receive an event sink".into())
    }

    async fn execute_with_event_sink(
        &self,
        run_id: RunId,
        _agent: String,
        input: String,
        mut history: Vec<Message>,
        _context: ToolContext,
        _cancellation: CancellationToken,
        event_sink: Arc<dyn EventSink>,
    ) -> Result<ExecutionReport, String> {
        event_sink.emit(&RuntimeEvent {
            run_id: run_id.as_uuid(),
            iteration: 1,
            kind: RuntimeEventKind::ToolExecutionStarted {
                tool_call_id: "tool-call-1".into(),
                name: "slow_tool".into(),
            },
        });
        self.started.notify_one();
        self.release.notified().await;

        history.push(Message::User { content: input });
        history.push(Message::Assistant {
            content: Some("done".into()),
            tool_calls: Vec::new(),
            model_id: Some("test/model".into()),
            generated_at: Some(chrono::Utc::now()),
        });
        Ok(ExecutionReport {
            outcome: ExecutionOutcome::Completed(Some("done".into())),
            runtime_events: Vec::new(),
            messages: Some(history),
        })
    }
}

#[tokio::test]
async fn runtime_events_are_durable_before_the_run_finishes() {
    let directory = std::env::temp_dir().join(format!("eme-live-events-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let database = directory.join("conversations.sqlite");
    let conversations = Arc::new(SqliteConversationStore::open(&database).unwrap());
    let conversation = conversations.create("owner", "assistant").unwrap();
    let execution = Arc::new(SqliteExecutionStore::open(&database).unwrap());
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());

    let manager = RunManager::new(
        Arc::new(LiveEventExecutor {
            started: started.clone(),
            release: release.clone(),
        }),
        1,
        2,
    )
    .unwrap()
    .with_storage(execution.clone());
    let run_id = manager
        .submit(
            "assistant",
            "prompt",
            Vec::new(),
            ToolContext {
                conversation_id: Some(conversation.id.to_string()),
                ..ToolContext::default()
            },
        )
        .await
        .unwrap();

    tokio::time::timeout(Duration::from_secs(2), started.notified())
        .await
        .expect("executor should start");
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let events = execution.list_events(run_id.as_uuid(), None).await.unwrap();
            if events.len() >= 3 {
                assert_eq!(events[2].kind, "tool_execution_started");
                assert_eq!(events[2].tool_name.as_deref(), Some("slow_tool"));
                assert_eq!(
                    manager.get(run_id).await.unwrap().status,
                    crate::RunStatus::Running
                );
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("runtime event should be persisted while execution is blocked");

    release.notify_one();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let completed = manager
                .get(run_id)
                .await
                .is_some_and(|run| run.status == crate::RunStatus::Completed);
            let events = execution.list_events(run_id.as_uuid(), None).await.unwrap();
            if completed && events.last().is_some_and(|event| event.kind == "completed") {
                assert_eq!(events.last().unwrap().sequence, 3);
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("run should complete after releasing the executor");

    drop(manager);
    drop(execution);
    drop(conversations);
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn scheduled_task_run_association_survives_manager_restart() {
    let directory = std::env::temp_dir().join(format!("eme-task-run-history-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let database = directory.join("executions.sqlite");
    let execution = Arc::new(SqliteExecutionStore::open(&database).unwrap());
    let task_id = Uuid::new_v4();

    let manager = RunManager::new(Arc::new(TranscriptExecutor), 1, 2)
        .unwrap()
        .with_storage(execution.clone());
    let run_id = manager
        .submit_for_task(
            task_id,
            "assistant",
            "scheduled prompt",
            Vec::new(),
            ToolContext::default(),
        )
        .await
        .unwrap();

    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if manager
                .get(run_id)
                .await
                .is_some_and(|run| run.status == crate::RunStatus::Completed)
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("scheduled run should complete");

    let runs = manager.task_runs(task_id).await.unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].id, run_id);
    assert_eq!(runs[0].response.as_deref(), Some("persisted answer"));

    drop(manager);
    drop(execution);
    let reopened = Arc::new(SqliteExecutionStore::open(&database).unwrap());
    let manager = RunManager::new(Arc::new(TranscriptExecutor), 1, 2)
        .unwrap()
        .with_storage(reopened);
    let runs = manager.task_runs(task_id).await.unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].id, run_id);
    assert_eq!(runs[0].status, crate::RunStatus::Completed);

    drop(manager);
    let _ = std::fs::remove_dir_all(directory);
}


#[tokio::test]
async fn persisted_running_run_can_be_cancelled_after_manager_restart() {
    let directory = std::env::temp_dir().join(format!("eme-restart-cancel-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let database = directory.join("conversations.sqlite");
    let conversations = SqliteConversationStore::open(&database).unwrap();
    let conversation = conversations.create("owner", "assistant").unwrap();
    let conversation_id = Uuid::parse_str(&conversation.id.to_string()).unwrap();
    let execution = Arc::new(SqliteExecutionStore::open(&database).unwrap());
    let run_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    execution.insert_run(StoredRun {
        id: run_id,
        conversation_id: Some(conversation_id),
        agent: "assistant".into(),
        input: "interrupted prompt".into(),
        status: StoredRunStatus::Running,
        created_at: now,
        started_at: Some(now),
        finished_at: None,
        response: None,
        error: None,
    }).await.unwrap();
    for (sequence, kind) in [(0, "queued"), (1, "started")] {
        execution.append_event(StoredRunEvent {
            run_id,
            sequence,
            kind: kind.into(),
            iteration: None,
            tool_name: None,
            model_id: None,
            next_model_id: None,
            model_candidates: None,
            attempted_candidates: None,
            error_category: None,
            occurred_at: now,
        }).await.unwrap();
    }

    // This manager has no worker or cancellation token for the persisted run.
    let manager = RunManager::new(Arc::new(TranscriptExecutor), 1, 2)
        .unwrap()
        .with_storage(execution.clone());
    let id = RunId::from_uuid(run_id);
    assert_eq!(manager.cancel(id).await, Ok(CancelRunOutcome::Requested));
    assert_eq!(manager.get(id).await.unwrap().status, crate::RunStatus::Cancelled);
    assert_eq!(manager.cancel(id).await, Ok(CancelRunOutcome::AlreadyFinished));
    let events = execution.list_events(run_id, None).await.unwrap();
    assert_eq!(events.last().unwrap().kind, "cancelled");

    drop(manager);
    drop(execution);
    drop(conversations);
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn deleting_conversation_waits_for_running_execution_cancellation() {
    let directory = std::env::temp_dir().join(format!("eme-delete-cancel-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let database = directory.join("conversations.sqlite");
    let conversations = Arc::new(SqliteConversationStore::open(&database).unwrap());
    let conversation = conversations.create("owner", "assistant").unwrap();
    let execution = Arc::new(SqliteExecutionStore::open(&database).unwrap());

    struct WaitForCancellation;
    #[async_trait]
    impl RunExecutor for WaitForCancellation {
        async fn execute(
            &self,
            _run_id: RunId,
            _agent: String,
            _input: String,
            _history: Vec<Message>,
            _context: ToolContext,
            cancellation: CancellationToken,
        ) -> Result<ExecutionReport, String> {
            cancellation.cancelled().await;
            Ok(ExecutionReport {
                outcome: ExecutionOutcome::Cancelled,
                runtime_events: Vec::new(),
                messages: None,
            })
        }
    }

    let manager = RunManager::new(Arc::new(WaitForCancellation), 1, 2)
        .unwrap()
        .with_storage(execution.clone());
    let run_id = manager.submit(
        "assistant",
        "running prompt",
        Vec::new(),
        ToolContext {
            conversation_id: Some(conversation.id.to_string()),
            ..ToolContext::default()
        },
    ).await.unwrap();

    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if manager.get(run_id).await.is_some_and(|run| run.status == crate::RunStatus::Running) {
                break;
            }
            tokio::task::yield_now().await;
        }
    }).await.expect("run should start");

    let conversation_uuid = Uuid::parse_str(&conversation.id.to_string()).unwrap();
    manager.cancel_conversation(conversation_uuid).await.unwrap();
    assert_eq!(manager.get(run_id).await.unwrap().status, crate::RunStatus::Cancelled);
    conversations.delete("owner", conversation.id).unwrap();
    assert!(conversations.get(conversation.id).is_err());

    drop(manager);
    drop(execution);
    drop(conversations);
    let _ = std::fs::remove_dir_all(directory);
}
