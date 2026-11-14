//! Alexa text-to-speech tool integration for the agent platform.
//!
//! This crate will expose an Alexa speech capability through the tool
//! interfaces defined by `agent-runtime`. It will encapsulate the Alexa
//! authentication, device-resolution, and speech-command interactions,
//! together with the model-facing tool definition.
//!
//! Agent execution, tool authorization, and conversation management will
//! remain the responsibility of the runtime.

mod alexa_speak;
mod tool;

use std::path::PathBuf;

use agent_runtime::{ToolDefinition, ToolRegistry};

pub fn register_tools(registry: &mut ToolRegistry) -> Result<(), String> {
    let definition_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/alexa_speak.toml");

    let contents = std::fs::read_to_string(&definition_path).map_err(|error| {
        format!(
            "Failed to read Alexa Speak tool definition {}: {error}",
            definition_path.display()
        )
    })?;

    let definition: ToolDefinition = toml::from_str(&contents)
        .map_err(|error| format!("Invalid Alexa Speak tool definition: {error}"))?;

    registry.register(definition, tool::AlexaSpeak)
}
