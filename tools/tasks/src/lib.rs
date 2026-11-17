//! Complete task lifecycle: TOML persistence, scheduling and agent tools.

mod registry;
mod scheduler;
mod tool;

pub use registry::{NewTask, Task, TaskError, TaskRegistry};
pub use scheduler::TaskScheduler;

use agent_config::ConfigError;
use agent_runtime::ToolRegistry;
use std::path::PathBuf;

pub fn register_tools(registry: &mut ToolRegistry, tasks: TaskRegistry) -> Result<(), ConfigError> {
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data");
    registry.register_from_file(data.join("task_list.toml"), tool::TaskList(tasks.clone()))?;
    registry.register_from_file(data.join("task_add.toml"), tool::TaskAdd(tasks.clone()))?;
    registry.register_from_file(data.join("task_remove.toml"), tool::TaskRemove(tasks))
}
