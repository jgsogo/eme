//! Errors shared

use thiserror::Error;

/// Errors shared by storage contracts and their implementations.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum StorageError {
    #[error("record already exists: {0}")]
    AlreadyExists(String),
    #[error("record not found: {0}")]
    NotFound(String),
    #[error("invalid record association: {0}")]
    InvalidAssociation(String),
    #[error("sequence {sequence} already exists for {stream}")]
    DuplicateSequence { stream: String, sequence: u64 },
    #[error("expected sequence {expected} for {stream}, got {actual}")]
    OutOfOrderSequence {
        stream: String,
        expected: u64,
        actual: u64,
    },
}
