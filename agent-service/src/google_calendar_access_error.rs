//! Errors returned while resolving an authorized Google Calendar for a tool call.

use thiserror::Error;

/// A fail-closed result from Google Calendar authorization.
#[derive(Debug, Error)]
pub enum GoogleCalendarAccessError {
    #[error("the requested calendar is unavailable")]
    NotFound,
    #[error("the caller is not authorized to read this calendar")]
    NotAuthorized,
    #[error("the authenticated EME user or calendar identifier is invalid")]
    InvalidIdentity,
    #[error("the Google connection does not grant calendar event read access")]
    InsufficientScopes,
    #[error("the caller does not have write permission for this calendar")]
    WriteNotAuthorized,
    #[error("the Google connection does not grant calendar event write access")]
    InsufficientWriteScopes,
    #[error("Google Calendar authorization storage failed: {0}")]
    Storage(String),
    #[error("family membership could not be verified")]
    FamilyUnavailable,
}
