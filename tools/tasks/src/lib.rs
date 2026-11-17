//! Agent task-management tools.
//!
//! This crate adapts the task service to the agent tool interface, exposing
//! operations to list, create and remove tasks.
//!
//! Ownership and persistence are handled by `agent-service`; each operation
//! is scoped to the authenticated user supplied in `ToolContext`.

mod tool;

use agent_config::ConfigError;
use agent_runtime::ToolRegistry;
use agent_service::TaskRegistry;
use std::path::PathBuf;

pub fn register_tools(registry: &mut ToolRegistry, tasks: TaskRegistry) -> Result<(), ConfigError> {
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data");
    registry.register_from_file(data.join("task_list.toml"), tool::TaskList(tasks.clone()))?;
    registry.register_from_file(data.join("task_add.toml"), tool::TaskAdd(tasks.clone()))?;
    registry.register_from_file(data.join("task_remove.toml"), tool::TaskRemove(tasks))
}
