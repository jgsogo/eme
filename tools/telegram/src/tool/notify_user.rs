use std::sync::Arc;

use crate::client::TelegramGateway;
use agent_runtime::{Tool, ToolContext, ToolError};
use async_trait::async_trait;
use serde_json::Value;

use super::{gateway_or_error, map_error};

/// `tg_notify_user` — send a one-way notification to a trusted conversation recipient.
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
    async fn execute(&self, context: &ToolContext, arguments: Value) -> Result<String, ToolError> {
        let message = arguments
            .get("message")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ToolError::InvalidArguments {
                tool_name: "tg_notify_user".into(),
                errors: vec!["Missing or invalid 'message' argument".into()],
            })?;

        let recipient = arguments.get("recipient").and_then(Value::as_str);
        let chat_id = super::resolve_chat_id(context, recipient)?;

        gateway_or_error(&self.gateway)?
            .notify_user(chat_id, message)
            .await
            .map_err(map_error)?;

        Ok("Notification sent.".to_owned())
    }
}
