//! Authenticates inbound messages and dispatches prompts to agent runs.

use std::{collections::HashSet, str::FromStr, sync::Arc};

use agent_runtime::ToolContext;
use agent_server::AgentListItem;
use agent_server::auth::TelegramAuth;
use agent_service::{FamilyService, FamilyServiceError, RunManager, SubmitRunError};
use agent_storage::{
    ConversationId, FamilyId, FamilyInvitationStore, FamilyRelationship, FamilyStore, FileUserIdentityStore,
    InvitationTargetIdentity, InvitationToken, SqliteConversationStore, SqliteFamilyStore, UserId,
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
    agents: &[AgentListItem],
    manager: &RunManager,
    store: &Arc<SqliteConversationStore>,
    identities: &Arc<FileUserIdentityStore>,
    families: &Arc<SqliteFamilyStore>,
    family_service: &Arc<FamilyService>,
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
        let (user, _) = match identities.resolve_or_create(
            "telegram",
            &telegram_user_id.to_string(),
            message.first_name.as_deref(),
        ) {
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
        let response = if auth.complete_login(
            token,
            telegram_user_id,
            message.first_name.as_deref(),
            identities.as_ref(),
        ) {
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
        match identities.resolve_or_create(
            "telegram",
            &telegram_user_id.to_string(),
            message.first_name.as_deref(),
        ) {
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
    if command == "/m" {
        if arguments.is_empty() {
            let _ = gateway.send_message(message.chat_id, "Usage: /m message").await;
        } else {
            handle_one_shot(
                gateway, &owner, &user, arguments, message.chat_id, message.message_id,
                telegram_user_id, manager, store, identities, family_service,
                active_conversations, auth,
            ).await;
        }
        return;
    }
    if handle_command(
        gateway,
        &owner,
        default_agent,
        agents,
        store,
        family_service,
        user.id,
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

    let _ = gateway
        .react_to_message(message.chat_id, message.message_id, "👀")
        .await;

    let conversation = match store.active(&owner) {
        Ok(Some(conversation)) => conversation,
        Ok(None) => {
            let _ = gateway
                .send_message(
                    message.chat_id,
                    "No current conversation. Create one with /new.",
                )
                .await;
            return;
        }
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

    let family = match conversation.selected_family_id {
        Some(family_id) => match family_service.context_for_member(user.id, family_id).await {
            Ok(mut context) => {
                for member in &mut context.members {
                    let member_id = match member.user_id.parse::<UserId>() {
                        Ok(id) => id,
                        Err(_) => {
                            active_conversations.lock().await.remove(&conversation.id);
                            let _ = gateway
                                .send_message(
                                    message.chat_id,
                                    "Could not resolve a family member identity.",
                                )
                                .await;
                            return;
                        }
                    };
                    member.telegram_id =
                        match identities.find_identity_for_user("telegram", member_id) {
                            Ok(identity) => identity.map(|identity| identity.subject),
                            Err(error) => {
                                active_conversations.lock().await.remove(&conversation.id);
                                send_error(
                                    gateway,
                                    message.chat_id,
                                    "load a family member Telegram identity",
                                    error,
                                )
                                .await;
                                return;
                            }
                        };
                    let user = match identities.get_user(member_id) {
                        Ok(user) => user,
                        Err(error) => {
                            active_conversations.lock().await.remove(&conversation.id);
                            send_error(gateway, message.chat_id, "load a family member", error)
                                .await;
                            return;
                        }
                    };
                    member.name = user.and_then(|user| user.display_name);
                }
                Some(context)
            }
            Err(FamilyServiceError::NotMember | FamilyServiceError::NotFound) => {
                active_conversations.lock().await.remove(&conversation.id);
                let _ = gateway.send_message(
                    message.chat_id,
                    "The family selected for this conversation is no longer available. Select another family in the web console.",
                ).await;
                return;
            }
            Err(error) => {
                active_conversations.lock().await.remove(&conversation.id);
                send_error(
                    gateway,
                    message.chat_id,
                    "resolve the selected family",
                    error,
                )
                .await;
                return;
            }
        },
        None => None,
    };

    // Resolve all current family memberships, not just the family selected
    // for this conversation, so the agent can notify any authorized relative.
    let mut family_contexts = match family_service.contexts_for_member(user.id).await {
        Ok(contexts) => contexts,
        Err(error) => {
            active_conversations.lock().await.remove(&conversation.id);
            send_error(
                gateway,
                message.chat_id,
                "resolve your family memberships",
                error,
            )
            .await;
            return;
        }
    };
    for family_context in &mut family_contexts {
        for member in &mut family_context.members {
            let member_id = match member.user_id.parse::<UserId>() {
                Ok(id) => id,
                Err(_) => {
                    active_conversations.lock().await.remove(&conversation.id);
                    let _ = gateway
                        .send_message(
                            message.chat_id,
                            "Could not resolve a family member identity.",
                        )
                        .await;
                    return;
                }
            };
            member.telegram_id = match identities.find_identity_for_user("telegram", member_id) {
                Ok(identity) => identity.map(|identity| identity.subject),
                Err(error) => {
                    active_conversations.lock().await.remove(&conversation.id);
                    send_error(
                        gateway,
                        message.chat_id,
                        "load a family member Telegram identity",
                        error,
                    )
                    .await;
                    return;
                }
            };
            let member_user = match identities.get_user(member_id) {
                Ok(user) => user,
                Err(error) => {
                    active_conversations.lock().await.remove(&conversation.id);
                    send_error(gateway, message.chat_id, "load a family member", error).await;
                    return;
                }
            };
            if let Some(member_user) = member_user {
                member.name = member_user.display_name;
                member.birthday = member_user.birthday;
            }
        }
    }

    match manager
        .submit(
            conversation.agent.clone(),
            prompt,
            conversation.messages.clone(),
            ToolContext {
                user_id: Some(user.id.to_string()),
                telegram_id: Some(telegram_user_id.to_string()),
                timezone: user.timezone.clone(),
                family,
                families: family_contexts,
                conversation_id: Some(conversation.id.to_string()),
                run_id: None,
            },
        )
        .await
    {
        Ok(id) => {
            tokio::spawn(report(
                gateway.clone(),
                message.chat_id,
                message.message_id,
                manager.clone(),
                store.clone(),
                conversation.id,
                active_conversations.clone(),
                id,
            ));
        }
        Err(SubmitRunError::Storage(error)) => {
            active_conversations.lock().await.remove(&conversation.id);
            tracing::error!(%error, conversation_id = %conversation.id, "failed to persist Telegram run");
            let _ = gateway
                .send_message(
                    message.chat_id,
                    "Could not persist this run; please try again.",
                )
                .await;
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


/// Starts a temporary run using the one-shot preferences saved in the user's profile.
async fn handle_one_shot(
    gateway: &Arc<TelegramGateway>,
    owner: &str,
    user: &agent_storage::User,
    message: &str,
    chat_id: i64,
    message_id: i32,
    telegram_user_id: i64,
    manager: &RunManager,
    store: &Arc<SqliteConversationStore>,
    identities: &Arc<FileUserIdentityStore>,
    family_service: &Arc<FamilyService>,
    active_conversations: &Arc<Mutex<HashSet<ConversationId>>>,
    auth: &Arc<TelegramAuth>,
) {
    let Some(agent) = user.one_shot_agent.as_deref().filter(|value| !value.trim().is_empty()) else {
        let _ = gateway.send_message(chat_id, "Configure a one-shot agent in your EME profile before using /m.").await;
        return;
    };
    let input = match user.one_shot_prompt.as_deref().filter(|value| !value.trim().is_empty()) {
        Some(prefix) => format!("{}\n\n{}", prefix.trim(), message.trim()),
        None => message.trim().to_owned(),
    };
    let family_id = match user.one_shot_family_id.as_deref() {
        Some(value) => match FamilyId::from_str(value) {
            Ok(id) => Some(id),
            Err(_) => {
                let _ = gateway.send_message(chat_id, "The family configured for one-shot execution is invalid. Update your profile settings.").await;
                return;
            }
        },
        None => None,
    };
    let family = match family_id {
        Some(id) => match family_service.context_for_member(user.id, id).await {
            Ok(mut context) => {
                for member in &mut context.members {
                    let member_id = match UserId::from_str(&member.user_id) {
                        Ok(id) => id,
                        Err(_) => {
                            let _ = gateway.send_message(chat_id, "Could not resolve a family member identity.").await;
                            return;
                        }
                    };
                    member.telegram_id = match identities.find_identity_for_user("telegram", member_id) {
                        Ok(identity) => identity.map(|identity| identity.subject),
                        Err(error) => {
                            send_error(gateway, chat_id, "load a family member Telegram identity", error).await;
                            return;
                        }
                    };
                    if let Ok(Some(member_user)) = identities.get_user(member_id) {
                        member.name = member_user.display_name;
                        member.birthday = member_user.birthday;
                    }
                }
                Some(context)
            }
            Err(FamilyServiceError::NotMember | FamilyServiceError::NotFound) => {
                let _ = gateway.send_message(chat_id, "The family configured for one-shot execution is no longer available. Update your profile settings.").await;
                return;
            }
            Err(error) => {
                send_error(gateway, chat_id, "resolve the one-shot family", error).await;
                return;
            }
        },
        None => None,
    };
    let conversation = match store.create(owner, agent.to_owned()) {
        Ok(conversation) => conversation,
        Err(error) => {
            send_error(gateway, chat_id, "prepare the one-shot execution", error).await;
            return;
        }
    };
    if let Err(error) = store.set_selected_family(owner, conversation.id, family_id) {
        let _ = store.delete(owner, conversation.id);
        send_error(gateway, chat_id, "associate the one-shot family", error).await;
        return;
    }
    active_conversations.lock().await.insert(conversation.id);
    let context = ToolContext {
        user_id: Some(user.id.to_string()),
        telegram_id: Some(telegram_user_id.to_string()),
        timezone: user.timezone.clone(),
        family,
        families: Vec::new(),
        conversation_id: Some(conversation.id.to_string()),
        run_id: None,
    };
    match manager.submit(agent.to_owned(), input, Vec::new(), context).await {
        Ok(run_id) => {
            auth.register_run(run_id.to_string(), owner);
            tokio::spawn(report(
                gateway.clone(), chat_id, message_id, manager.clone(), store.clone(),
                conversation.id, active_conversations.clone(), run_id,
            ));
            let cleanup_store = store.clone();
            let cleanup_manager = manager.clone();
            let cleanup_owner = owner.to_owned();
            let conversation_id = conversation.id;
            tokio::spawn(async move {
                loop {
                    if cleanup_manager.get(run_id).await.is_some_and(|run| run.status.is_terminal()) {
                        if let Err(error) = cleanup_store.delete(&cleanup_owner, conversation_id) {
                            tracing::warn!(%conversation_id, %error, "failed to remove Telegram one-shot conversation");
                        }
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            });
        }
        Err(SubmitRunError::QueueFull) => {
            active_conversations.lock().await.remove(&conversation.id);
            let _ = store.delete(owner, conversation.id);
            let _ = gateway.send_message(chat_id, "The agent queue is full; try again shortly.").await;
        }
        Err(SubmitRunError::Storage(error)) => {
            active_conversations.lock().await.remove(&conversation.id);
            let _ = store.delete(owner, conversation.id);
            send_error(gateway, chat_id, "persist the one-shot execution", error).await;
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
            first_name: Some("Javier".to_owned()),
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
