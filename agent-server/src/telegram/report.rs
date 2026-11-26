//! Waits for Telegram-triggered runs and delivers their final results.

use std::{collections::HashSet, sync::Arc, time::Duration};

use agent_service::{RunId, RunManager, RunStatus};
use agent_storage::{ConversationId, SqliteConversationStore};
use telegram_tool::TelegramGateway;
use tokio::sync::Mutex;

use super::formatting::truncate;

pub(super) async fn report(
    gateway: Arc<TelegramGateway>,
    chat_id: i64,
    message_id: i32,
    manager: RunManager,
    _store: Arc<SqliteConversationStore>,
    conversation_id: ConversationId,
    active_conversations: Arc<Mutex<HashSet<ConversationId>>>,
    id: RunId,
) {
    loop {
        match manager.get(id).await {
            Some(run) if run.status.is_terminal() => {
                let text = match run.status {
                    RunStatus::Completed => run
                        .response
                        .unwrap_or_else(|| "Completed without a text response.".into()),
                    RunStatus::Failed => {
                        format!("Execution failed: {}", run.error.unwrap_or_default())
                    }
                    RunStatus::Cancelled => "Execution cancelled.".into(),
                    RunStatus::TimedOut => "Execution timed out.".into(),
                    RunStatus::MaxIterationsReached => "Maximum iterations reached.".into(),
                    _ => unreachable!(),
                };

                active_conversations.lock().await.remove(&conversation_id);
                if let Err(error) = gateway.send_message(chat_id, &truncate(&text)).await {
                    eprintln!("Telegram result delivery failed: {error}");
                } else if let Err(error) = gateway
                    .clear_message_reaction(chat_id, message_id)
                    .await
                {
                    eprintln!("Telegram reaction cleanup failed: {error}");
                }

                return;
            }
            Some(_) => tokio::time::sleep(Duration::from_millis(500)).await,
            None => return,
        }
    }
}
