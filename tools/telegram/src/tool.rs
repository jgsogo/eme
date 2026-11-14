use crate::client::{TelegramClient, default_timeout};
use agent_runtime::{Tool, ToolError};
use async_trait::async_trait;
use serde_json::Value;

/// `tg_notify_user` — send a one-way notification to the configured chat.
pub struct TgNotifyUser;

#[async_trait]
impl Tool for TgNotifyUser {
    async fn execute(&self, arguments: Value) -> Result<String, ToolError> {
        let message = arguments
            .get("message")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ToolError::InvalidArguments {
                tool_name: "tg_notify_user".into(),
                errors: vec!["Missing or invalid 'message' argument".into()],
            })?;

        let client = TelegramClient::from_env()
            .map_err(|e| ToolError::Configuration(e.to_string()))?;
        client
            .notify(message)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        Ok("Notification sent.".to_owned())
    }
}

/// `tg_ask_user` — ask the user a question and wait for the answer.
///
/// Arguments:
/// - `question` (string, required): the text shown to the user
/// - `kind` (string, optional): `"text"` (default) or `"yes_no"`
pub struct TgAskUser;

#[async_trait]
impl Tool for TgAskUser {
    async fn execute(&self, arguments: Value) -> Result<String, ToolError> {
        let question = arguments
            .get("question")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ToolError::InvalidArguments {
                tool_name: "tg_ask_user".into(),
                errors: vec!["Missing or invalid 'question' argument".into()],
            })?;

        let kind = arguments
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("text");

        let client = TelegramClient::from_env()
            .map_err(|e| ToolError::Configuration(e.to_string()))?;
        let timeout = default_timeout();

        match kind {
            "text" => {
                let answer = client
                    .ask_text(question, timeout)
                    .await
                    .map_err(|e| ToolError::Execution(e.to_string()))?;
                Ok(answer)
            }
            "yes_no" => {
                let answer = client
                    .ask_yes_no(question, timeout)
                    .await
                    .map_err(|e| ToolError::Execution(e.to_string()))?;
                Ok(if answer {
                    "yes".to_owned()
                } else {
                    "no".to_owned()
                })
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
