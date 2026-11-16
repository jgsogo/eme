//! Authenticated browser session state and its expiry.

use std::time::Instant;

pub(super) struct Session {
    pub(super) owner: String,
    pub(super) expires_at: Instant,
}
