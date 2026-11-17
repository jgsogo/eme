//! Tool type for removing a task owned by the current user.
use agent_service::TaskRegistry;
pub(in crate::tool) struct TaskRemove(pub(in crate::tool) TaskRegistry);
