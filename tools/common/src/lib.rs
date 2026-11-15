//! Generic tools that expose information available from the local runtime.

mod datetime;

use std::path::PathBuf;

use agent_config::ConfigError;
use agent_runtime::ToolRegistry;

/// Register all generic tools.
pub fn register_tools(registry: &mut ToolRegistry) -> Result<(), ConfigError> {
    let definition_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/current_datetime.toml");
    registry.register_from_file(definition_path, datetime::CurrentDateTime)
}
