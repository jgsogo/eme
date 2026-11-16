use std::{collections::HashSet, sync::Arc};

use agent_runtime::ToolContext;
use agent_server::auth::TelegramAuth;
use agent_service::{RunManager, SubmitRunError};
use agent_storage::{
    ConversationId, ConversationStore, FamilyInvitationStore, FamilyRelationship,
    FileUserIdentityStore, InvitationTargetIdentity, InvitationToken, SqliteFamilyStore,
    UserIdentityStore,
};
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
    identities: &Arc<FileUserIdentityStore>,
    families: &Arc<SqliteFamilyStore>,
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
    let Some(telegram_user_id) = message.user_id else {
        return;
    };
    let Some(prompt) = message
        .text
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
    else {
        return;
    };

    let mut parts = prompt.splitn(2, char::is_whitespace);
    let command = parts.next().unwrap_or_default();
    let arguments = parts.next().unwrap_or_default().trim();

    // Invitation payloads are deliberately handled before the configured
    // allow-list: a valid invitation is itself the authorization to join.
    // Require a private chat so bearer tokens are not exposed to a group.
    if command == "/start" && arguments.len() == 64 {
        if message.chat_id != telegram_user_id {
            let _ = gateway
                .send_message(
                    message.chat_id,
                    "Please open the invitation in a private chat with this bot.",
                )
                .await;
            return;
        }
        let token = match InvitationToken::parse(arguments) {
            Ok(token) => token,
            Err(_) => {
                let _ = gateway
                    .send_message(
                        message.chat_id,
                        "This family invitation is invalid or has expired.",
                    )
                    .await;
                return;
            }
        };
        let identity = match InvitationTargetIdentity::new("telegram", telegram_user_id.to_string())
        {
            Ok(identity) => identity,
            Err(error) => {
                send_error(
                    gateway,
                    message.chat_id,
                    "prepare your Telegram identity",
                    error,
                )
                .await;
                return;
            }
        };

        if families
            .validate_invitation(&token, &identity)
            .await
            .is_err()
        {
            let _ = gateway
                .send_message(message.chat_id, "This family invitation is invalid, expired, revoked, or belongs to another Telegram account.")
                .await;
            return;
        }

        // Only provision an EME account after the bearer token and optional
        // Telegram identity binding have been checked.
        let (user, _) =
            match identities.resolve_or_create("telegram", &telegram_user_id.to_string(), None) {
                Ok(identity) => identity,
                Err(error) => {
                    send_error(gateway, message.chat_id, "create your EME identity", error).await;
                    return;
                }
            };

        match families
            .accept_invitation(&token, &identity, user.id, FamilyRelationship::Other)
            .await
        {
            Ok(member) => {
                let _ = gateway
                    .send_message(
                        message.chat_id,
                        &format!("You have joined family {} successfully. Send a message to start using EME.", member.family_id),
                    )
                    .await;
            }
            Err(_) => {
                let _ = gateway
                    .send_message(message.chat_id, "The invitation could not be accepted. It may have already been used, revoked, or expired.")
                    .await;
            }
        }
        return;
    }

    if command == "/start" && arguments.starts_with("auth_") {
        if message.chat_id != telegram_user_id {
            let _ = gateway
                .send_message(
                    message.chat_id,
                    "Please complete web login in a private chat with this bot.",
                )
                .await;
            return;
        }
        let token = arguments.trim_start_matches("auth_");
        let response = if auth.complete_login(token, telegram_user_id, identities.as_ref()) {
            "Web login approved. You can return to the EME console."
        } else {
            "This login link is invalid or has expired. Please start again from the EME console."
        };
        let _ = gateway.send_message(message.chat_id, response).await;
        return;
    }

    // Existing configured users remain supported. Other users may use the bot
    // only after their Telegram identity has an active family membership.
    let existing_identity =
        match identities.find_identity("telegram", &telegram_user_id.to_string()) {
            Ok(identity) => identity,
            Err(error) => {
                send_error(gateway, message.chat_id, "look up your EME identity", error).await;
                return;
            }
        };
    let user = if allowed.contains(&telegram_user_id) {
        match identities.resolve_or_create("telegram", &telegram_user_id.to_string(), None) {
            Ok((user, _)) => Some(user),
            Err(error) => {
                send_error(gateway, message.chat_id, "resolve your EME identity", error).await;
                return;
            }
        }
    } else if let Some(identity) = existing_identity {
        match families.list_families(identity.user_id).await {
            Ok(family_list) if !family_list.is_empty() => {
                match identities.get_user(identity.user_id) {
                    Ok(user) => user,
                    Err(error) => {
                        send_error(gateway, message.chat_id, "load your EME identity", error).await;
                        return;
                    }
                }
            }
            _ => None,
        }
    } else {
        None
    };
    let Some(user) = user else {
        warn!(telegram_user_id, "Rejected unauthorized Telegram user");
        let _ = gateway
            .send_message(message.chat_id, "You are not authorized to use EME.")
            .await;
        return;
    };

    let owner = format!("telegram:{telegram_user_id}");
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
                user_id: Some(user.id.to_string()),
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
