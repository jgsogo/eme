//! Tool type for creating a task owned by the current user.
use crate::TaskRegistry;
pub(crate) struct TaskAdd(pub(crate) TaskRegistry);
