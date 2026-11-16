use std::time::Instant;

pub(super) struct Session {
    owner: String,
    expires_at: Instant,
}
