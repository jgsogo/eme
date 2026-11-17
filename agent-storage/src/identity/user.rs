//! Platform user record.

use super::UserId;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// Stable internal EME identity. Provider-specific identifiers belong in UserIdentity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub display_name: Option<String>,
    /// Calendar date of birth, when known.
    #[serde(default)]
    pub birthday: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
