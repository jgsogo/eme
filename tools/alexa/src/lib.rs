//! Alexa integration tools for the agent platform.
//!
//! This crate will provide Alexa-specific tool implementations, including
//! authentication, device resolution, and Alexa API interactions. Individual
//! tools are registered through the interfaces exposed by `agent-runtime`.
//!
//! Agent execution, tool authorization, and conversation management remain
//! the responsibility of the runtime.

mod alexa_speak;
mod tool;

use std::path::PathBuf;

use agent_config::{ConfigError, read_toml};
use agent_runtime::{ToolDefinition, ToolRegistry};

pub fn register_tools(registry: &mut ToolRegistry) -> Result<(), ConfigError> {
    let definition_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/alexa_speak.toml");
    let definition: ToolDefinition = read_toml(&definition_path)?;
    registry.register(definition, tool::AlexaSpeak)
}
