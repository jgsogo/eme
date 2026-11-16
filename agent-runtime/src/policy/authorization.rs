//! Authorization decision returned by a policy engine.

use super::AuthorizationError;

/// Result of an authorization decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Authorization {
    Allowed,
    Denied(AuthorizationError),
}
