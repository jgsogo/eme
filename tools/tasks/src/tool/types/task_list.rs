//! Tool type for listing tasks owned by the current user.
use agent_service::TaskRegistry;
pub(super) struct TaskList(pub(super) TaskRegistry);
