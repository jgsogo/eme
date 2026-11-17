//! Tool type for listing tasks owned by the current user.
use agent_service::TaskRegistry;
pub(in crate::tool) struct TaskList(pub(in crate::tool) TaskRegistry);
