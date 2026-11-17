//! Tool type for removing a task owned by the current user.
use agent_service::TaskRegistry;
pub(crate) struct TaskRemove(pub(crate) TaskRegistry);
