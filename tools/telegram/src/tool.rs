use std::sync::Arc;

use crate::client::{TelegramError, TelegramGateway, configured_chat_id, default_timeout};
use agent_runtime::{Tool, ToolError};
use async_trait::async_trait;
use serde_json::Value;

fn gateway_or_error(
    gateway: &Option<Arc<TelegramGateway>>,
) -> Result<&Arc<TelegramGateway>, ToolError> {
    gateway.as_ref().ok_or_else(|| {
        ToolError::Configuration(
            "Telegram gateway is not configured (set TELEGRAM_BOT_TOKEN)".into(),
        )
    })
}

fn map_error(error: TelegramError) -> ToolError {
    match error {
        error @ (TelegramError::MissingToken | TelegramError::MissingChatId) => {
            ToolError::Configuration(error.to_string())
        }
        error => ToolError::Execution(error.to_string()),
    }
}

/// `tg_notify_user` — send a one-way notification to the configured chat.
pub struct TgNotifyUser {
    gateway: Option<Arc<TelegramGateway>>,
}

impl TgNotifyUser {
    pub fn new(gateway: Option<Arc<TelegramGateway>>) -> Self {
        Self { gateway }
    }
}

#[async_trait]
impl Tool for TgNotifyUser {
    async fn execute(&self, arguments: Value) -> Result<String, ToolError> {
        let message = arguments
            .get("message")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ToolError::InvalidArguments {
                tool_name: "tg_notify_user".into(),
                errors: vec!["Missing or invalid 'message' argument".into()],
            })?;

        gateway_or_error(&self.gateway)?
            .notify_configured_chat(message)
            .await
            .map_err(map_error)?;

        Ok("Notification sent.".to_owned())
    }
}

/// `tg_ask_user` — ask the configured user and wait for the gateway to route the reply.
pub struct TgAskUser {
    gateway: Option<Arc<TelegramGateway>>,
}

impl TgAskUser {
    pub fn new(gateway: Option<Arc<TelegramGateway>>) -> Self {
        Self { gateway }
    }
}

#[async_trait]
impl Tool for TgAskUser {
    async fn execute(&self, arguments: Value) -> Result<String, ToolError> {
        let question = arguments
            .get("question")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ToolError::InvalidArguments {
                tool_name: "tg_ask_user".into(),
                errors: vec!["Missing or invalid 'question' argument".into()],
            })?;
        let kind = arguments
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("text");
        let chat_id = configured_chat_id().map_err(map_error)?;
        let gateway = gateway_or_error(&self.gateway)?;
        let timeout = default_timeout();

        match kind {
            "text" => gateway
                .ask_text(chat_id, question, timeout)
                .await
                .map_err(map_error),
            "yes_no" => {
                let answer = gateway
                    .ask_yes_no(chat_id, question, timeout)
                    .await
                    .map_err(map_error)?;
                Ok(if answer { "yes" } else { "no" }.to_owned())
            }
            other => Err(ToolError::InvalidArguments {
                tool_name: "tg_ask_user".into(),
                errors: vec![format!(
                    "Invalid 'kind' value '{other}'. Expected 'text' or 'yes_no'."
                )],
            }),
        }
    }
}
