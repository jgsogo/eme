//! Tool type for listing tasks owned by the current user.
use agent_service::TaskRegistry;
pub(crate) struct TaskList(pub(crate) TaskRegistry);
