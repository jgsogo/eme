use thiserror::Error;

/// Errors returned when a run cannot be accepted.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SubmitRunError {
    #[error("the run queue is full")]
    QueueFull,
}
