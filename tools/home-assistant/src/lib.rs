//! Home Assistant integration for the EME agent platform.
//!
//! This crate provides narrowly-scoped tools for reading Home Assistant state,
//! discovering available services, and invoking explicitly requested services.
//! Each operation is registered as a separate tool so an agent's existing
//! allow-list policy can grant read access without granting control access.
//!
//! The crate owns HTTP transport, authentication, and Home Assistant API
//! details. Agent policy and user-facing orchestration remain responsibilities
//! of the runtime and host application.

mod client;
mod error;
mod operation;
mod tool;

use std::path::PathBuf;

use agent_config::ConfigError;
use agent_runtime::ToolRegistry;

use operation::Operation;
use tool::HomeAssistantTool;

/// Register all Home Assistant tools.
pub fn register_tools(registry: &mut ToolRegistry) -> Result<(), ConfigError> {
    let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data");
    for (definition, operation) in [
        ("get_states.toml", Operation::GetStates),
        ("get_state.toml", Operation::GetState),
        ("get_services.toml", Operation::GetServices),
        ("call_service.toml", Operation::CallService),
        ("get_history.toml", Operation::GetHistory),
        ("get_config.toml", Operation::GetConfig),
    ] {
        registry
            .register_from_file(data_dir.join(definition), HomeAssistantTool::new(operation))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use agent_runtime::ToolRegistry;

    use super::register_tools;

    #[test]
    fn registers_separate_read_and_control_tools() {
        let mut registry = ToolRegistry::new();
        register_tools(&mut registry).unwrap();

        for name in [
            "home_assistant.get_states",
            "home_assistant.get_state",
            "home_assistant.get_services",
            "home_assistant.call_service",
            "home_assistant.get_history",
            "home_assistant.get_config",
        ] {
            assert!(registry.contains(name), "missing tool {name}");
        }
    }
}
