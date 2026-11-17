//! Tool type for removing a task owned by the current user.
use crate::TaskRegistry;
pub(crate) struct TaskRemove(pub(crate) TaskRegistry);
