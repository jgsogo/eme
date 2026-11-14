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
