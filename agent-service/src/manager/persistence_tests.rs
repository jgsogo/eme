use super::*;
use crate::RunId;
use agent_runtime::{CancellationToken, Message, ToolContext};
use agent_storage::{
    ConversationMessageStore, RunEventStore, RunStore, SqliteConversationStore,
    SqliteExecutionStore,
};
use async_trait::async_trait;
use std::{sync::Arc, time::Duration};
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
