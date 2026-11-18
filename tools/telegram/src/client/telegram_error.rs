use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TelegramError {
    #[error("TELEGRAM_BOT_TOKEN env var not set")]
    MissingToken,
    #[error("Telegram API error: {0}")]
    Api(String),
    #[error("Timed out waiting for user reply after {0:?}")]
    Timeout(Duration),
    #[error("Telegram update dispatcher is closed")]
    DispatcherClosed,
}
