//! #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Family

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::UserId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Family {
    pub id: FamilyId,
    pub name: String,
    /// Historical attribution; this user need not remain a member.
    pub creator_id: UserId,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
