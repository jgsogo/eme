//! Telegram inbound adapter. It consumes events from the shared gateway;
//! it never polls Telegram directly.

use std::{collections::HashSet, sync::Arc, time::Duration};

use agent_service::{RunId, RunManager, RunStatus, SubmitRunError};
use agent_storage::{ConversationId, ConversationStore};
use anyhow::{Context, Result};
use telegram_tool::{IncomingMessage, TelegramGateway};
use tokio::sync::{Mutex, broadcast};

pub fn allowed_users_from_env() -> Result<HashSet<i64>> {
    let value = std::env::var("TELEGRAM_ALLOWED_USERS")
        .context("TELEGRAM_ALLOWED_USERS must be set when Telegram is enabled")?;
    let ids = value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .parse::<i64>()
                .with_context(|| format!("invalid Telegram user id '{value}'"))
        })
        .collect::<Result<HashSet<_>>>()?;

    if ids.is_empty() {
        anyhow::bail!("TELEGRAM_ALLOWED_USERS must contain at least one user id");
    }

    Ok(ids)
}

fn truncate(text: &str) -> String {
    if text.chars().count() <= 3500 {
        return text.to_owned();
    }

    format!(
        "{}\n\n[Response truncated]",
        text.chars().take(3500).collect::<String>()
    )
}

async fn report(
    gateway: Arc<TelegramGateway>,
    chat_id: i64,
    manager: RunManager,
    store: Arc<ConversationStore>,
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

                if let Some(messages) = manager.history(id).await {
                    if let Err(error) = store.save_messages(conversation_id, messages) {
                        eprintln!("Telegram conversation persistence failed: {error}");
                    }
                }

                active_conversations.lock().await.remove(&conversation_id);
                if let Err(error) = gateway.send_message(chat_id, &truncate(&text)).await {
                    eprintln!("Telegram result delivery failed: {error}");
                }

                return;
            }
            Some(_) => tokio::time::sleep(Duration::from_millis(500)).await,
            None => return,
        }
    }
}

async fn handle_message(
    gateway: &Arc<TelegramGateway>,
    allowed: &HashSet<i64>,
    agent: &str,
    manager: &RunManager,
    store: &Arc<ConversationStore>,
    active_conversations: &Arc<Mutex<HashSet<ConversationId>>>,
    message: IncomingMessage,
) {
    if message.is_bot {
        return;
    }

    let Some(user_id) = message.user_id else {
        return;
    };
    let Some(prompt) = message
        .text
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
    else {
        return;
    };

    if !allowed.contains(&user_id) {
        let _ = gateway
            .send_message(message.chat_id, "You are not authorized to use this bot.")
            .await;
        return;
    }

    let owner = format!("telegram:{}:{}", message.chat_id, user_id);

    if prompt == "/new" {
        match store.create(owner, agent) {
            Ok(conversation) => {
                let _ = gateway
                    .send_message(
                        message.chat_id,
                        &format!(
                            "Started a new conversation ({}) with agent '{agent}'.",
                            conversation.id
                        ),
                    )
                    .await;
            }
            Err(error) => {
                let _ = gateway
                    .send_message(
                        message.chat_id,
                        &format!("Could not create conversation: {error}"),
                    )
                    .await;
            }
        }
        return;
    }

    if prompt == "/start" || prompt == "/help" {
        let _ = gateway
            .send_message(
                message.chat_id,
                &format!(
                    "Send a prompt to chat with the '{agent}' agent. Use /new to start a new conversation."
                ),
            )
            .await;
        return;
    }

    let conversation = match store.active(&owner) {
        Ok(Some(conversation)) => conversation,
        Ok(None) => match store.create(owner, agent) {
            Ok(conversation) => conversation,
            Err(error) => {
                let _ = gateway
                    .send_message(
                        message.chat_id,
                        &format!("Could not create conversation: {error}"),
                    )
                    .await;
                return;
            }
        },
        Err(error) => {
            let _ = gateway
                .send_message(
                    message.chat_id,
                    &format!("Could not load conversation: {error}"),
                )
                .await;
            return;
        }
    };

    {
        let mut active = active_conversations.lock().await;
        if active.contains(&conversation.id) {
            let _ = gateway
                .send_message(
                    message.chat_id,
                    "This conversation already has a run in progress. Please wait for its response or use /new to start another conversation.",
                )
                .await;
            return;
        }
        active.insert(conversation.id);
    }

    match manager
        .submit_with_history(agent.to_owned(), prompt, conversation.messages.clone())
        .await
    {
        Ok(id) => {
            let _ = gateway
                .send_message(message.chat_id, &format!("Run {id} queued."))
                .await;
            tokio::spawn(report(
                gateway.clone(),
                message.chat_id,
                manager.clone(),
                store.clone(),
                conversation.id,
                active_conversations.clone(),
                id,
            ));
        }
        Err(SubmitRunError::QueueFull) => {
            active_conversations.lock().await.remove(&conversation.id);
            let _ = gateway
                .send_message(
                    message.chat_id,
                    "The agent queue is full; try again shortly.",
                )
                .await;
        }
    }
}

pub async fn run_bot(
    gateway: Arc<TelegramGateway>,
    allowed: HashSet<i64>,
    agent: String,
    manager: RunManager,
    store: Arc<ConversationStore>,
    mut updates: broadcast::Receiver<IncomingMessage>,
) {
    let active_conversations = Arc::new(Mutex::new(HashSet::new()));
    loop {
        match updates.recv().await {
            Ok(message) => {
                handle_message(
                    &gateway,
                    &allowed,
                    &agent,
                    &manager,
                    &store,
                    &active_conversations,
                    message,
                )
                .await;
            }
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                eprintln!("Telegram dispatcher lagged; skipped {skipped} messages");
            }
            Err(broadcast::error::RecvError::Closed) => {
                eprintln!("Telegram update dispatcher closed");
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn truncates_long_text() {
        let text = "x".repeat(4000);
        assert!(super::truncate(&text).ends_with("[Response truncated]"));
    }

    #[test]
    fn truncation_preserves_short_text() {
        assert_eq!(super::truncate("hello"), "hello");
    }
}
