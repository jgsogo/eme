mod tool;

use std::path::PathBuf;

use agent_runtime::{ToolRegistry, ToolDefinition};

pub fn register_tools(registry: &mut ToolRegistry) -> Result<(), String> {
    let definition_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("data/alexa.toml");

    let contents = std::fs::read_to_string(&definition_path)
        .map_err(|error| {
            format!(
                "Failed to read Alexa tool definition {}: {error}",
                definition_path.display()
            )
        })?;

    let definition: ToolDefinition = toml::from_str(&contents)
        .map_err(|error| format!("Invalid Alexa tool definition: {error}"))?;

    registry.register(definition, tool::AlexaTool)
}

