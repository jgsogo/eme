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

async fn send_error(
    gateway: &TelegramGateway,
    chat_id: i64,
    action: &str,
    error: impl std::fmt::Display,
) {
    let _ = gateway
        .send_message(chat_id, &format!("Could not {action}: {error}"))
        .await;
}

async fn handle_command(
    gateway: &Arc<TelegramGateway>,
    owner: &str,
    default_agent: &str,
    agents: &[String],
    store: &Arc<ConversationStore>,
    chat_id: i64,
    command: &str,
    arguments: &str,
) -> bool {
    match command {
        "/start" | "/help" => {
            let _ = gateway
                .send_message(
                    chat_id,
                    &format!(
                        "Commands:\n/new — start a fresh conversation\n/history — list conversations\n/use <id> — resume a conversation\n/agents — list agents\n/agent <name> — start a conversation with an agent\n\nDefault agent: '{default_agent}'."
                    ),
                )
                .await;
            true
        }
        "/new" if arguments.is_empty() => {
            let agent = match store.active(owner) {
                Ok(Some(conversation)) => conversation.agent,
                Ok(None) => default_agent.to_owned(),
                Err(error) => {
                    send_error(gateway, chat_id, "load the active conversation", error).await;
                    return true;
                }
            };
            match store.create(owner, agent.clone()) {
                Ok(conversation) => {
                    let _ = gateway
                        .send_message(
                            chat_id,
                            &format!(
                                "Started a new conversation ({}) with agent '{agent}'.",
                                conversation.id
                            ),
                        )
                        .await;
                }
                Err(error) => send_error(gateway, chat_id, "create a conversation", error).await,
            }
            true
        }
        "/history" if arguments.is_empty() => {
            let conversations = match store.list(owner) {
                Ok(conversations) => conversations,
                Err(error) => {
                    send_error(gateway, chat_id, "list conversations", error).await;
                    return true;
                }
            };
            if conversations.is_empty() {
                let _ = gateway
                    .send_message(
                        chat_id,
                        "No conversations yet. Send a message to start one.",
                    )
                    .await;
                return true;
            }
            let active_id = match store.active(owner) {
                Ok(conversation) => conversation.map(|conversation| conversation.id),
                Err(error) => {
                    send_error(gateway, chat_id, "load the active conversation", error).await;
                    return true;
                }
            };
            let mut lines = vec!["Conversations (newest activity first):".to_owned()];
            for conversation in conversations.iter().take(10) {
                let marker = if Some(conversation.id) == active_id {
                    " [active]"
                } else {
                    ""
                };
                lines.push(format!(
                    "{}{} — {} — {} — {} messages",
                    conversation.id,
                    marker,
                    conversation.agent,
                    conversation.updated_at.format("%Y-%m-%d %H:%M UTC"),
                    conversation.messages.len()
                ));
            }
            if conversations.len() > 10 {
                lines.push(format!(
                    "{} older conversations omitted.",
                    conversations.len() - 10
                ));
            }
            lines.push("Use /use <conversation-id> to continue one.".into());
            let _ = gateway
                .send_message(chat_id, &truncate(&lines.join("\n")))
                .await;
            true
        }
        "/use" => {
            let Ok(id) = arguments.parse::<ConversationId>() else {
                let _ = gateway
                    .send_message(chat_id, "Usage: /use <conversation-id>. See /history.")
                    .await;
                return true;
            };
            match store.set_active(owner, id) {
                Ok(()) => match store.get(id) {
                    Ok(conversation) => {
                        let _ = gateway
                            .send_message(
                                chat_id,
                                &format!(
                                    "Continuing conversation {} with '{}' ({} messages).",
                                    conversation.id,
                                    conversation.agent,
                                    conversation.messages.len()
                                ),
                            )
                            .await;
                    }
                    Err(error) => {
                        send_error(gateway, chat_id, "load the conversation", error).await
                    }
                },
                Err(error) => send_error(gateway, chat_id, "select the conversation", error).await,
            }
            true
        }
        "/agents" if arguments.is_empty() => {
            let active_agent = match store.active(owner) {
                Ok(conversation) => conversation.map(|conversation| conversation.agent),
                Err(error) => {
                    send_error(gateway, chat_id, "load the active conversation", error).await;
                    return true;
                }
            };
            let mut lines = vec!["Configured agents:".to_owned()];
            for agent in agents {
                let marker = if active_agent.as_deref() == Some(agent.as_str()) {
                    " [active]"
                } else {
                    ""
                };
                lines.push(format!("- {agent}{marker}"));
            }
            lines.push("Use /agent <name> to start a new conversation.".into());
            let _ = gateway.send_message(chat_id, &truncate(&lines.join("\n"))).await;
            true
        }
        "/agent" => {
            let Some(agent) = agents.iter().find(|agent| agent.as_str() == arguments) else {
                let _ = gateway
                    .send_message(
                        chat_id,
                        "Unknown agent. Use /agents to list available agents.",
                    )
                    .await;
                return true;
            };
            match store.create(owner, agent.clone()) {
                Ok(conversation) => {
                    let _ = gateway
                        .send_message(
                            chat_id,
                            &format!(
                                "Started a new conversation ({}) with agent '{}'. Previous conversations are unchanged.",
                                conversation.id, agent
                            ),
                        )
                        .await;
                }
                Err(error) => send_error(gateway, chat_id, "start a conversation", error).await,
            }
            true
        }
        _ => false,
    }
}

async fn handle_message(
    gateway: &Arc<TelegramGateway>,
    allowed: &HashSet<i64>,
    default_agent: &str,
    agents: &[String],
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
    let mut parts = prompt.splitn(2, char::is_whitespace);
    let command = parts.next().unwrap_or_default();
    let arguments = parts.next().unwrap_or_default().trim();
    if handle_command(
        gateway,
        &owner,
        default_agent,
        agents,
        store,
        message.chat_id,
        command,
        arguments,
    )
    .await
    {
        return;
    }
    if command.starts_with('/') {
        let _ = gateway
            .send_message(message.chat_id, "Unknown command. Use /help.")
            .await;
        return;
    }

    let conversation = match store.active(&owner) {
        Ok(Some(conversation)) => conversation,
        Ok(None) => match store.create(&owner, default_agent) {
            Ok(conversation) => conversation,
            Err(error) => {
                send_error(gateway, message.chat_id, "create a conversation", error).await;
                return;
            }
        },
        Err(error) => {
            send_error(
                gateway,
                message.chat_id,
                "load the active conversation",
                error,
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
                    "This conversation already has a run in progress. Please wait or use /new.",
                )
                .await;
            return;
        }
        active.insert(conversation.id);
    }

    match manager
        .submit_with_history(
            conversation.agent.clone(),
            prompt,
            conversation.messages.clone(),
        )
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
    agents: Vec<String>,
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
                    &agents,
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
