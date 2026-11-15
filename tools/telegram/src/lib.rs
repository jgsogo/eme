//! Shared Telegram integration for tools and inbound message adapters.
//!
//! A single TelegramGateway owns update polling. Replies to interactive tool
//! questions are routed by Telegram message ID; all other messages are
//! broadcast to inbound adapters.

mod client;
mod tool;

use std::{path::PathBuf, sync::Arc};

use agent_config::ConfigError;
use agent_runtime::ToolRegistry;

pub use client::{IncomingMessage, TelegramError, TelegramGateway};

/// Register Telegram tools. The gateway is shared with the server's inbound
/// message handler; when absent, tool calls fail with a configuration error.
pub fn register_tools(
    registry: &mut ToolRegistry,
    gateway: Option<Arc<TelegramGateway>>,
) -> Result<(), ConfigError> {
    let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data");

    registry.register_from_file(
        data_dir.join("tg_notify_user.toml"),
        tool::TgNotifyUser::new(gateway.clone()),
    )?;
    registry.register_from_file(
        data_dir.join("tg_ask_user.toml"),
        tool::TgAskUser::new(gateway),
    )?;

    Ok(())
}
