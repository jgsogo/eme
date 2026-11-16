//! Errors returned when cancelling a run.

use thiserror::Error;

/// Errors returned when cancellation targets an unknown run.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CancelRunError {
    #[error("run not found")]
    NotFound,
}
