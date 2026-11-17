//! Tool type for creating a task owned by the current user.
use agent_service::TaskRegistry;
pub(crate) struct TaskAdd(pub(crate) TaskRegistry);
