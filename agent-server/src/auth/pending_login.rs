use std::time::Instant;

pub(super) struct PendingLogin {
    expires_at: Instant,
    session: Option<String>,
}
