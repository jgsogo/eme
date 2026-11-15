use std::sync::Arc;

use crate::client::TelegramGateway;
use agent_runtime::{Tool, ToolError};
use async_trait::async_trait;
use serde_json::Value;

use super::{gateway_or_error, map_error};

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
