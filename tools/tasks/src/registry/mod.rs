//! User-scoped task definitions and file-backed persistence.

mod inner;
mod new_task;
mod stored_task;
mod task;
mod task_error;
mod task_registry;
#[cfg(test)]
mod tests;

pub use new_task::NewTask;
pub use task::Task;
pub use task_error::TaskError;
pub use task_registry::TaskRegistry;

use inner::Inner;
use stored_task::StoredTask;

const LEGACY_TASK_OWNER: &str = "telegram:118716073";

fn default_enabled() -> bool {
    true
}
