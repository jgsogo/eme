//! Failures returned by identity persistence.

use super::UserId;
use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IdentityStoreError {
    #[error("identity storage I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("identity storage serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("user {0} was not found")]
    UserNotFound(UserId),
    #[error("identity ({provider}, {subject}) is already linked to another user")]
    IdentityAlreadyLinked { provider: String, subject: String },
    #[error("identity provider and subject must not be empty")]
    EmptyIdentity,
    #[error("display name must contain between 1 and 80 characters")]
    InvalidDisplayName,
}
