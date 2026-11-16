//! Invitation typeExternal identity

use serde::{Deserialize, Serialize};
use super::InvitationStorageError;

/// External identity an invitation is optionally restricted to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvitationTargetIdentity {
    pub provider: String,
    pub subject: String,
}

impl InvitationTargetIdentity {
    pub fn new(
        provider: impl Into<String>,
        subject: impl Into<String>,
    ) -> Result<Self, InvitationStorageError> {
        let provider = provider.into();
        let subject = subject.into();
        if provider.trim().is_empty() || subject.trim().is_empty() {
            return Err(InvitationStorageError::EmptyTargetIdentity);
        }
        Ok(Self { provider, subject })
    }
}
