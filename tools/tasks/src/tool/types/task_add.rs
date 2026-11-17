//! Tool type for creating a task owned by the current user.
use agent_service::TaskRegistry;
pub(in crate::tool) struct TaskAdd(pub(in crate::tool) TaskRegistry);
