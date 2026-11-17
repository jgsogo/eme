//! Errors returned by note persistence and access checks.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum NotesError {
    #[error("note not found")]
    NotFound,
    #[error("invalid note: {0}")]
    Invalid(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("family storage error: {0}")]
    Family(String),
}
