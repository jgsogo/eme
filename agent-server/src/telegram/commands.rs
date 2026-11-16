//! Telegram command parsing and conversation-management commands.

use std::sync::Arc;

use agent_storage::{ConversationId, ConversationStore};
use telegram_tool::TelegramGateway;

use super::formatting::{send_error, truncate};

pub(super) async fn handle_command(
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
            let _ = gateway
                .send_message(chat_id, &truncate(&lines.join("\n")))
                .await;
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
