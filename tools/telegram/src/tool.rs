mod ask_user;
mod notify_user;

pub use ask_user::TgAskUser;
pub use notify_user::TgNotifyUser;

use std::sync::Arc;

use crate::client::{TelegramError, TelegramGateway};
use agent_runtime::ToolError;

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
