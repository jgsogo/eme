//! State retained while a Telegram login challenge is awaiting approval.

use std::time::Instant;

pub(super) struct PendingLogin {
    pub(super) expires_at: Instant,
    pub(super) session: Option<String>,
}
