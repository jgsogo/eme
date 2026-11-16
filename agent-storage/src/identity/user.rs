//! Platform user record.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use super::UserId;

/// Stable internal EME identity. Provider-specific identifiers belong in UserIdentity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
