//! Tavily web search integration for the agent platform.
//!
//! The crate exposes a provider-backed web search tool. Tavily-specific HTTP
//! details stay inside this crate; the agent runtime only sees the registered
//! tool definition and its JSON result.

mod client;
mod tool;

use std::path::PathBuf;

use agent_config::ConfigError;
use agent_runtime::ToolRegistry;

/// Register the Tavily web search tool.
pub fn register_tools(registry: &mut ToolRegistry) -> Result<(), ConfigError> {
    let definition_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/web_search.toml");
    registry.register_from_file(definition_path, tool::WebSearch)
}
