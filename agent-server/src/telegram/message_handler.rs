use std::{collections::HashSet, sync::Arc};

use agent_runtime::ToolContext;
use agent_server::auth::TelegramAuth;
use agent_service::{RunManager, SubmitRunError};
use agent_storage::{ConversationId, ConversationStore};
use telegram_tool::{IncomingMessage, TelegramGateway};
use tokio::sync::Mutex;
use tracing::warn;

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
    // Replies are reserved for interactive Telegram tool calls. If the gateway
    // did not match one to a pending question (e.g. it expired), never turn it
    // into a fresh agent prompt.
    if !is_new_prompt(&message) {
        warn!(
            chat_id = message.chat_id,
            message_id = message.message_id,
            reply_to_message_id = message.reply_to_message_id,
            "Ignoring Telegram reply without a matching pending question"
        );
        return;
    }

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
        warn!(
            telegram_user_id = user_id,
            "Rejected unauthorized Telegram user"
        );
        let _ = gateway
            .send_message(message.chat_id, "You are not authorized to use EME.")
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
        .submit(
            conversation.agent.clone(),
            prompt,
            conversation.messages.clone(),
            ToolContext {
                user_id: Some(owner.clone()),
                conversation_id: Some(conversation.id.to_string()),
                run_id: None,
            },
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

/// Only non-reply messages from a user can start a new inbound run.
fn is_new_prompt(message: &IncomingMessage) -> bool {
    !message.is_bot && message.user_id.is_some() && message.reply_to_message_id.is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(reply_to_message_id: Option<i32>) -> IncomingMessage {
        IncomingMessage {
            chat_id: 42,
            user_id: Some(7),
            is_bot: false,
            message_id: 200,
            reply_to_message_id,
            text: Some("hello".to_owned()),
        }
    }

    #[test]
    fn reply_messages_are_not_new_prompts() {
        assert!(!is_new_prompt(&message(Some(100))));
    }

    #[test]
    fn ordinary_user_messages_can_start_new_prompts() {
        assert!(is_new_prompt(&message(None)));
    }

    #[test]
    fn bot_messages_cannot_start_new_prompts() {
        let mut message = message(None);
        message.is_bot = true;
        assert!(!is_new_prompt(&message));
    }

    #[test]
    fn messages_without_a_user_id_cannot_start_new_prompts() {
        let mut message = message(None);
        message.user_id = None;
        assert!(!is_new_prompt(&message));
    }
}
