//! Tool type for creating a task owned by the current user.
use agent_service::TaskRegistry;
pub(super) struct TaskAdd(pub(super) TaskRegistry);
