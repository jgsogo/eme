//! Telegram integration tools for the agent platform.
//!
//! This crate provides Telegram-specific tool implementations for short
//! human interactions: asking the user a question (and waiting for the
//! answer) and sending notifications. Individual tools are registered
//! through the interfaces exposed by `agent-runtime`.
//!
//! Agent execution, tool authorization, and conversation management remain
//! the responsibility of the runtime.

mod client;
mod tool;

use std::path::PathBuf;

use agent_config::ConfigError;
use agent_runtime::ToolRegistry;

/// Register all Telegram tools into the given registry.
pub fn register_tools(registry: &mut ToolRegistry) -> Result<(), ConfigError> {
    let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data");

    registry.register_from_file(data_dir.join("tg_notify_user.toml"), tool::TgNotifyUser)?;
    registry.register_from_file(data_dir.join("tg_ask_user.toml"), tool::TgAskUser)?;

    Ok(())
}
