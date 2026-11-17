//! Tool type for listing tasks owned by the current user.
use crate::TaskRegistry;
pub(crate) struct TaskList(pub(crate) TaskRegistry);
