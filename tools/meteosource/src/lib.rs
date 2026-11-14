//! Meteosource weather integration for the agent platform.
//!
//! The crate exposes a tool that resolves a place name and retrieves its
//! current weather and hourly forecast from the Meteosource API.

mod tool;
mod weather;

use std::path::PathBuf;

use agent_config::ConfigError;
use agent_runtime::ToolRegistry;

/// Register the Meteosource weather tool.
pub fn register_tools(registry: &mut ToolRegistry) -> Result<(), ConfigError> {
    let definition_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/meteosource.toml");
    registry.register_from_file(definition_path, tool::MeteosourceWeather)
}
