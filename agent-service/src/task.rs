//! Scheduled task definitions and registry access.

mod task_registry;

pub use task_registry::{LEGACY_TASK_OWNER, NewTask, Task, TaskError, TaskRegistry};
