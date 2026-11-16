//! External identity linked to an internal user.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use super::UserId;

/// An external identity linked to an EME user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserIdentity {
    pub provider: String,
    pub subject: String,
    pub user_id: UserId,
    pub created_at: DateTime<Utc>,
}
