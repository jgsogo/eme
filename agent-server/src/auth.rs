//! Telegram-backed authentication and browser session management.

//! Telegram-based web authentication and browser sessions.

mod routes;
mod telegram_auth;

pub use routes::auth_api;
pub use telegram_auth::TelegramAuth;
