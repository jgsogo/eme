//! Server-only credentials and provider routing details after authorization.

use agent_storage::ConnectedCalendar;

/// Data required by the Google adapter after a successful authorization check.
///
/// This type is internal to trusted server code. Do not serialize it, return it
/// from an API, or pass it to the model.
pub struct AuthorizedGoogleCalendar {
    pub calendar: ConnectedCalendar,
    pub refresh_token: String,
}
