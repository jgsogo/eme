//! Shared helpers and exports for Telegram tool adapters.

mod ask_user;
mod notify_user;

pub use ask_user::TgAskUser;
pub use notify_user::TgNotifyUser;

use std::sync::Arc;

use crate::client::{TelegramError, TelegramGateway};
use agent_runtime::{ToolContext, ToolError};

fn gateway_or_error(
    gateway: &Option<Arc<TelegramGateway>>,
) -> Result<&Arc<TelegramGateway>, ToolError> {
    gateway.as_ref().ok_or_else(|| {
        ToolError::Configuration(
            "Telegram gateway is not configured (set TELEGRAM_BOT_TOKEN)".into(),
        )
    })
}

fn resolve_chat_id(context: &ToolContext, recipient: Option<&str>) -> Result<i64, ToolError> {
    let telegram_id = match recipient {
        None => context.telegram_id.as_deref().ok_or_else(|| {
            ToolError::Configuration(
                "The conversation owner has no linked Telegram account.".into(),
            )
        })?,
        Some(recipient_id) => {
            let member = context
                .family
                .as_ref()
                .and_then(|family| {
                    family
                        .members
                        .iter()
                        .find(|member| member.user_id == recipient_id)
                })
                .ok_or_else(|| {
                    ToolError::Execution(format!(
                        "Recipient '{recipient_id}' is not a member of the conversation family."
                    ))
                })?;
            member.telegram_id.as_deref().ok_or_else(|| {
                ToolError::Execution(format!(
                    "Family member '{recipient_id}' has no linked Telegram account."
                ))
            })?
        }
    };

    telegram_id
        .parse::<i64>()
        .map_err(|_| ToolError::Configuration("The resolved Telegram identity is invalid.".into()))
}

fn map_error(error: TelegramError) -> ToolError {
    match error {
        error @ TelegramError::MissingToken => ToolError::Configuration(error.to_string()),
        error => ToolError::Execution(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use agent_runtime::{FamilyContext, FamilyMemberContext, ToolContext};

    use super::resolve_chat_id;

    fn family_context() -> FamilyContext {
        FamilyContext {
            family_id: "family-1".into(),
            family_name: "Home".into(),
            family_role: "owner".into(),
            relationship: "father".into(),
            members: vec![
                FamilyMemberContext {
                    user_id: "owner-user-id".into(),
                    name: Some("Owner".into()),
                    birthday: None,
                    telegram_id: Some("42".into()),
                    role: "owner".into(),
                    relationship: "father".into(),
                },
                FamilyMemberContext {
                    user_id: "member-user-id".into(),
                    name: Some("Member".into()),
                    birthday: None,
                    telegram_id: Some("84".into()),
                    role: "member".into(),
                    relationship: "daughter".into(),
                },
            ],
        }
    }

    #[test]
    fn defaults_to_the_authenticated_conversation_owner() {
        let context = ToolContext {
            telegram_id: Some("42".into()),
            ..ToolContext::default()
        };

        assert_eq!(resolve_chat_id(&context, None).unwrap(), 42);
    }

    #[test]
    fn resolves_only_members_of_the_trusted_family_context() {
        let context = ToolContext {
            telegram_id: Some("42".into()),
            family: Some(family_context()),
            ..ToolContext::default()
        };

        assert_eq!(
            resolve_chat_id(&context, Some("member-user-id")).unwrap(),
            84
        );
    }

    #[test]
    fn rejects_recipients_outside_the_trusted_family_context() {
        let context = ToolContext {
            telegram_id: Some("42".into()),
            family: Some(family_context()),
            ..ToolContext::default()
        };

        assert!(resolve_chat_id(&context, Some("unknown-user-id")).is_err());
    }

    #[test]
    fn rejects_family_members_without_a_linked_telegram_account() {
        let mut family = family_context();
        family.members[1].telegram_id = None;
        let context = ToolContext {
            family: Some(family),
            ..ToolContext::default()
        };

        assert!(resolve_chat_id(&context, Some("member-user-id")).is_err());
    }

    #[test]
    fn rejects_missing_owner_telegram_identity() {
        assert!(resolve_chat_id(&ToolContext::default(), None).is_err());
    }
}
