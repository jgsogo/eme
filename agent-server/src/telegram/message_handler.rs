use std::{collections::HashSet, sync::Arc};

use agent_runtime::ToolContext;
use agent_server::auth::TelegramAuth;
use agent_service::{RunManager, SubmitRunError};
use agent_storage::{
    ConversationId, ConversationStore, FamilyInvitationStore, FamilyRelationship, FamilyStore,
    FileUserIdentityStore, InvitationStorageError, InvitationTargetIdentity, InvitationToken,
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
    families: &Arc<dyn FamilyStore>,
    invitations: &Arc<dyn FamilyInvitationStore>,
    active_conversations: &Arc<Mutex<HashSet<ConversationId>>>,
    auth: &Arc<TelegramAuth>,
    message: IncomingMessage,
) {
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
    let Some(prompt) = message.text.map(|text| text.trim().to_owned()).filter(|text| !text.is_empty()) else {
        return;
    };

    let mut parts = prompt.splitn(2, char::is_whitespace);
    let command = parts.next().unwrap_or_default();
    let arguments = parts.next().unwrap_or_default().trim();

    // Invitations are accepted in a private chat, and are deliberately handled
    // before the ordinary allow-list check so a first-time recipient can join.
    if command == "/start" {
        if message.chat_id != telegram_user_id {
            let response = if arguments.starts_with("invite_") {
                "Please open this invitation in a private chat with the bot."
            } else {
                "Please complete this action in a private chat with the bot."
            };
            let _ = gateway.send_message(message.chat_id, response).await;
            return;
        }
        if let Some(token) = invitation_token_from_start(arguments) {
            accept_telegram_invitation(
                gateway,
                telegram_user_id,
                &token,
                identities,
                families,
                invitations,
                message.chat_id,
            )
            .await;
            return;
        }
    }

    if !allowed.contains(&telegram_user_id) {
        // An accepted family member is also allowed to use the bot. This is
        // checked against server-side membership, never request/model input.
        let existing_user = identities
            .find_identity("telegram", &telegram_user_id.to_string())
            .ok()
            .flatten()
            .and_then(|identity| identities.get_user(identity.user_id).ok().flatten());
        let has_family_membership = match existing_user {
            Some(user) => families
                .list_families(user.id)
                .await
                .is_ok_and(|memberships| !memberships.is_empty()),
            None => false,
        };
        if !has_family_membership {
            warn!(telegram_user_id, "Rejected unauthorized Telegram user");
            let _ = gateway.send_message(message.chat_id, "You are not authorized to use EME.").await;
            return;
        }
    }

    if command == "/start" && arguments.starts_with("auth_") {
        let token = arguments.trim_start_matches("auth_");
        let response = if auth.complete_login(token, telegram_user_id, identities.as_ref()) {
            "Web login approved. You can return to the EME console."
        } else {
            "This login link is invalid or has expired. Please start again from the EME console."
        };
        let _ = gateway.send_message(message.chat_id, response).await;
        return;
    }

    let (user, _) = match identities.resolve_or_create("telegram", &telegram_user_id.to_string(), None) {
        Ok(identity) => identity,
        Err(error) => {
            send_error(gateway, message.chat_id, "resolve your EME identity", error).await;
            return;
        }
    };
    let owner = format!("telegram:{telegram_user_id}");
    if handle_command(
        gateway, &owner, default_agent, agents, store, message.chat_id, command, arguments,
    ).await {
        return;
    }
    if command.starts_with('/') {
        let _ = gateway.send_message(message.chat_id, "Unknown command. Use /help.").await;
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
            send_error(gateway, message.chat_id, "load the active conversation", error).await;
            return;
        }
    };

    {
        let mut active = active_conversations.lock().await;
        if active.contains(&conversation.id) {
            let _ = gateway.send_message(message.chat_id, "This conversation already has a run in progress. Please wait or use /new.").await;
            return;
        }
        active.insert(conversation.id);
    }

    match manager.submit(
        conversation.agent.clone(),
        prompt,
        conversation.messages.clone(),
        ToolContext {
            user_id: Some(user.id.to_string()),
            conversation_id: Some(conversation.id.to_string()),
            run_id: None,
        },
    ).await {
        Ok(id) => {
            let _ = gateway.send_message(message.chat_id, &format!("Run {id} queued.")).await;
            tokio::spawn(report(
                gateway.clone(), message.chat_id, manager.clone(), store.clone(),
                conversation.id, active_conversations.clone(), id,
            ));
        }
        Err(SubmitRunError::QueueFull) => {
            active_conversations.lock().await.remove(&conversation.id);
            let _ = gateway.send_message(message.chat_id, "The agent queue is full; try again shortly.").await;
        }
    }
}

async fn accept_telegram_invitation(
    gateway: &Arc<TelegramGateway>,
    telegram_user_id: i64,
    token: &InvitationToken,
    identities: &Arc<FileUserIdentityStore>,
    families: &Arc<dyn FamilyStore>,
    invitations: &Arc<dyn FamilyInvitationStore>,
    chat_id: i64,
) {
    let external_identity = match InvitationTargetIdentity::new("telegram", telegram_user_id.to_string()) {
        Ok(identity) => identity,
        Err(_) => {
            let _ = gateway.send_message(chat_id, "This invitation is invalid.").await;
            return;
        }
    };

    // Reuse an existing identity for an idempotent retry after acceptance.
    let existing_identity = identities
        .find_identity("telegram", &telegram_user_id.to_string())
        .ok()
        .flatten();
    let existing_user = existing_identity
        .as_ref()
        .and_then(|identity| identities.get_user(identity.user_id).ok().flatten());

    // Validate first: invalid/expired/revoked links must not provision accounts.
    let validation = invitations.validate_invitation(token, &external_identity).await;
    if let Err(error) = validation {
        if !matches!(error, InvitationStorageError::AlreadyAccepted) || existing_user.is_none() {
            let _ = gateway.send_message(chat_id, invitation_error_message(&error)).await;
            return;
        }
    }

    let resolved = match (existing_user, existing_identity) {
        (Some(user), Some(identity)) => Ok((user, identity)),
        _ => identities.resolve_or_create("telegram", &telegram_user_id.to_string(), None),
    };
    let (user, _) = match resolved {
        Ok(identity) => identity,
        Err(error) => {
            send_error(gateway, chat_id, "create your EME identity", error).await;
            return;
        }
    };

    match invitations.accept_invitation(
        token,
        &external_identity,
        user.id,
        FamilyRelationship::Other,
    ).await {
        Ok(member) => {
            let family_name = families.get_family(member.family_id).await
                .ok()
                .flatten()
                .map(|family| family.name)
                .unwrap_or_else(|| "your family".to_owned());
            let _ = gateway.send_message(
                chat_id,
                &format!("You have joined {family_name}! Send a message to start using EME."),
            ).await;
        }
        Err(error) => {
            let _ = gateway.send_message(chat_id, invitation_error_message(&error)).await;
        }
    }
}

fn invitation_error_message(error: &InvitationStorageError) -> &'static str {
    match error {
        InvitationStorageError::Expired => "This invitation has expired. Ask a family member for a new invitation.",
        InvitationStorageError::Revoked => "This invitation has been revoked.",
        InvitationStorageError::AlreadyAccepted => "This invitation has already been used.",
        InvitationStorageError::IdentityMismatch => "This invitation was issued for a different Telegram account.",
        InvitationStorageError::NotFound => "This invitation link is invalid.",
        _ => "The invitation could not be accepted. Please ask for a new invitation.",
    }
}

fn invitation_token_from_start(arguments: &str) -> Option<InvitationToken> {
    let payload = arguments.strip_prefix("invite_")?;
    InvitationToken::parse(payload)
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
    fn parses_only_valid_invitation_start_payloads() {
        assert!(invitation_token_from_start(&format!("invite_{}", "a".repeat(64))).is_some());
        assert!(invitation_token_from_start("invite_short").is_none());
        assert!(invitation_token_from_start(&format!("invite_{}", "g".repeat(64))).is_none());
        assert!(invitation_token_from_start("auth_123").is_none());
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
