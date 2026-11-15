use std::{collections::HashSet, sync::Arc};

use agent_server::auth::TelegramAuth;
use agent_service::{RunManager, SubmitRunError};
use agent_storage::{ConversationId, ConversationStore};
use telegram_tool::{IncomingMessage, TelegramGateway};
use tokio::sync::Mutex;

use super::{commands::handle_command, formatting::send_error, report::report};

pub(super) async fn handle_message(
    gateway: &Arc<TelegramGateway>,
    allowed: &HashSet<i64>,
    default_agent: &str,
    agents: &[String],
    manager: &RunManager,
    store: &Arc<ConversationStore>,
    active_conversations: &Arc<Mutex<HashSet<ConversationId>>>,
    auth: &Arc<TelegramAuth>,
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

    let mut parts = prompt.splitn(2, char::is_whitespace);
    let command = parts.next().unwrap_or_default();
    let arguments = parts.next().unwrap_or_default().trim();

    if command == "/start" && arguments.starts_with("auth_") {
        if message.chat_id != user_id {
            let _ = gateway
                .send_message(
                    message.chat_id,
                    "Please complete web login in a private chat with this bot.",
                )
                .await;
            return;
        }
        let token = arguments.trim_start_matches("auth_");
        let response = if auth.complete_login(token, user_id) {
            "Web login approved. You can return to the EME console."
        } else {
            "This login link is invalid or has expired. Please start again from the EME console."
        };
        let _ = gateway.send_message(message.chat_id, response).await;
        return;
    }

    let owner = format!("telegram:{user_id}");
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
