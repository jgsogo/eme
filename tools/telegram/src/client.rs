#[path = "client/gateway.rs"]
mod gateway;
#[path = "client/incoming_message.rs"]
mod incoming_message;
#[path = "client/pending_reply.rs"]
mod pending_reply;
use pending_reply::PendingReply;
#[path = "client/telegram_error.rs"]
mod telegram_error;

pub use gateway::{TelegramGateway, configured_chat_id, default_timeout};
pub use incoming_message::IncomingMessage;
pub use telegram_error::TelegramError;
