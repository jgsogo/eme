//! Errors returned by task validation and persistence operations.

#[derive(Debug, thiserror::Error)]
pub enum TaskError {
    #[error("task not found")]
    NotFound,
    #[error("invalid task: {0}")]
    Invalid(String),
    #[error("task storage error: {0}")]
    Storage(String),
}
