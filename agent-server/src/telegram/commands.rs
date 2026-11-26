//! Telegram command parsing and conversation-management commands.

use std::sync::Arc;

use agent_runtime::Message;
use agent_server::AgentListItem;
use agent_service::FamilyService;
use agent_storage::{SqliteConversationStore, UserId};
use telegram_tool::{TelegramGateway, default_timeout};

use super::formatting::{send_error, truncate};

pub(super) async fn handle_command(
    gateway: &Arc<TelegramGateway>,
    owner: &str,
    default_agent: &str,
    agents: &[AgentListItem],
    store: &Arc<SqliteConversationStore>,
    family_service: &Arc<FamilyService>,
    user_id: UserId,
    chat_id: i64,
    command: &str,
    arguments: &str,
) -> bool {
    match command {
        "/help" => {
            let _ = gateway.send_message(chat_id,
                "Available commands:\n/help — show this help\n/new — create a conversation\n/info — show the current conversation\n/agents — browse configured agents").await;
            true
        }
        "/new" => {
            create_conversation(
                gateway,
                owner,
                default_agent,
                agents,
                store,
                family_service,
                user_id,
                chat_id,
            )
            .await;
            true
        }
        "/info" => {
            let conversation = match store.active(owner) {
                Ok(Some(value)) => value,
                Ok(None) => {
                    let _ = gateway
                        .send_message(chat_id, "No current conversation. Create one with /new.")
                        .await;
                    return true;
                }
                Err(error) => {
                    send_error(gateway, chat_id, "load the current conversation", error).await;
                    return true;
                }
            };
            let family = match conversation.selected_family_id {
                Some(id) => family_service
                    .list_families(user_id)
                    .await
                    .ok()
                    .and_then(|items| {
                        items
                            .into_iter()
                            .find(|item| item.id == id)
                            .map(|item| item.name)
                    })
                    .unwrap_or_else(|| id.to_string()),
                None => "None".to_owned(),
            };
            let info = format!(
                "Current conversation\n\nName: {}\nAgent: {}\nFamily: {}\nMessages: {}\nEstimated tokens: ~{}\nCreated: {}\nLast activity: {}\nID: {}",
                conversation.title,
                conversation.agent,
                family,
                conversation.messages.len(),
                estimate_tokens(&conversation.messages),
                conversation.created_at.format("%Y-%m-%d %H:%M UTC"),
                conversation.updated_at.format("%Y-%m-%d %H:%M UTC"),
                conversation.id
            );
            let _ = gateway.send_message(chat_id, &truncate(&info)).await;
            true
        }
        "/agents" => {
            if agents.is_empty() {
                let _ = gateway
                    .send_message(chat_id, "No agents are configured.")
                    .await;
                return true;
            }
            let active = match store.active(owner) {
                Ok(value) => value.map(|conversation| conversation.agent),
                Err(error) => {
                    send_error(gateway, chat_id, "load the current conversation", error).await;
                    return true;
                }
            };
            let mut lines = vec!["Configured agents — select one to see its details:".to_owned()];
            let buttons = agents
                .iter()
                .enumerate()
                .map(|(index, agent)| {
                    let marker = if active.as_deref() == Some(agent.name.as_str()) {
                        " ✓"
                    } else {
                        ""
                    };
                    lines.push(format!("• {}{}", agent.name, marker));
                    (
                        format!("{}{}", agent.name, marker),
                        format!("/agent_details {}", index),
                    )
                })
                .collect::<Vec<_>>();
            let rows = buttons.chunks(2).map(|chunk| chunk.to_vec()).collect();
            let _ = gateway
                .send_message_with_buttons(chat_id, &truncate(&lines.join("\n")), rows)
                .await;
            true
        }
        "/agent_details" => {
            let Some(index) = arguments.parse::<usize>().ok() else {
                let _ = gateway
                    .send_message(chat_id, "Invalid agent selection. Use /agents.")
                    .await;
                return true;
            };
            let Some(agent) = agents.get(index) else {
                let _ = gateway
                    .send_message(chat_id, "Invalid agent selection. Use /agents.")
                    .await;
                return true;
            };
            let details = format!(
                "{}\n\n{}\n\nCapabilities: {:?}\nTools: {}",
                agent.name,
                agent.description,
                agent.capabilities,
                if agent.tools.is_empty() {
                    "None".to_owned()
                } else {
                    agent.tools.join(", ")
                }
            );
            let _ = gateway.send_message(chat_id, &truncate(&details)).await;
            true
        }
        _ => false,
    }
}

async fn create_conversation(
    gateway: &Arc<TelegramGateway>,
    owner: &str,
    default_agent: &str,
    agents: &[AgentListItem],
    store: &Arc<SqliteConversationStore>,
    family_service: &Arc<FamilyService>,
    user_id: UserId,
    chat_id: i64,
) {
    let title = match gateway
        .ask_text(
            chat_id,
            "What would you like to name this conversation? Reply with a name, or 'cancel'.",
            default_timeout(),
        )
        .await
    {
        Ok(value) if !value.trim().is_empty() && !value.trim().eq_ignore_ascii_case("cancel") => {
            value.trim().to_owned()
        }
        Ok(_) => {
            let _ = gateway
                .send_message(chat_id, "Conversation creation cancelled.")
                .await;
            return;
        }
        Err(error) => {
            send_error(gateway, chat_id, "get the conversation name", error).await;
            return;
        }
    };

    let available = agents
        .iter()
        .map(|agent| agent.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let question = format!(
        "Which agent should handle this conversation?\nAvailable agents: {available}\nReply with an exact name, or 'cancel'."
    );
    let agent = match gateway
        .ask_text(chat_id, &question, default_timeout())
        .await
    {
        Ok(value) if value.trim().eq_ignore_ascii_case("cancel") => {
            let _ = gateway
                .send_message(chat_id, "Conversation creation cancelled.")
                .await;
            return;
        }
        Ok(value) if value.trim().is_empty() => default_agent.to_owned(),
        Ok(value) => value.trim().to_owned(),
        Err(error) => {
            send_error(gateway, chat_id, "get the agent selection", error).await;
            return;
        }
    };
    if !agents.iter().any(|item| item.name == agent) {
        let _ = gateway
            .send_message(
                chat_id,
                "Unknown agent. Creation cancelled; use /agents to browse agents.",
            )
            .await;
        return;
    }

    let families = match family_service.list_families(user_id).await {
        Ok(value) => value,
        Err(error) => {
            send_error(gateway, chat_id, "list your families", error).await;
            return;
        }
    };
    let selected_family = if families.is_empty() {
        None
    } else {
        let names = families
            .iter()
            .map(|family| family.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let question = format!(
            "Which family should be associated with this conversation?\nAvailable families: {names}\nReply with an exact family name, or 'none' for no family, or 'cancel'."
        );
        let answer = match gateway
            .ask_text(chat_id, &question, default_timeout())
            .await
        {
            Ok(value) => value.trim().to_owned(),
            Err(error) => {
                send_error(gateway, chat_id, "get the family selection", error).await;
                return;
            }
        };
        if answer.eq_ignore_ascii_case("cancel") {
            let _ = gateway
                .send_message(chat_id, "Conversation creation cancelled.")
                .await;
            return;
        }
        if answer.is_empty() || answer.eq_ignore_ascii_case("none") {
            None
        } else {
            match families
                .iter()
                .find(|family| family.name.eq_ignore_ascii_case(&answer))
            {
                Some(family) => Some(family.id),
                None => {
                    let _ = gateway
                        .send_message(chat_id, "Unknown family. Creation cancelled.")
                        .await;
                    return;
                }
            }
        }
    };

    match store.create(owner, agent.clone()) {
        Ok(conversation) => {
            if let Err(error) = store.rename(owner, conversation.id, title.clone()) {
                send_error(gateway, chat_id, "name the conversation", error).await;
                return;
            }
            if let Err(error) = store.set_selected_family(owner, conversation.id, selected_family) {
                send_error(gateway, chat_id, "associate the family", error).await;
                return;
            }
            let family_name = selected_family
                .and_then(|id| {
                    families
                        .iter()
                        .find(|family| family.id == id)
                        .map(|family| family.name.clone())
                })
                .unwrap_or_else(|| "None".to_owned());
            let _ = gateway.send_message(chat_id,
                &format!("Created conversation '{}'.\nAgent: {}\nFamily: {}\n\nSend your first message whenever you're ready.", title, agent, family_name)).await;
        }
        Err(error) => send_error(gateway, chat_id, "create a conversation", error).await,
    }
}

fn estimate_tokens(messages: &[Message]) -> usize {
    let chars = messages
        .iter()
        .map(|message| match message {
            Message::User { content } => content.chars().count(),
            Message::Assistant { content, .. } => {
                content.as_deref().unwrap_or_default().chars().count()
            }
            Message::Tool { name, content, .. } => name.chars().count() + content.chars().count(),
        })
        .sum::<usize>();
    chars.div_ceil(4)
}
