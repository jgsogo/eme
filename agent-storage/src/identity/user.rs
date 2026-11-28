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
    /// IANA timezone identifier configured for this user, when known.
    #[serde(default)]
    pub timezone: Option<String>,
    /// Agent used for one-shot executions initiated from any client.
    #[serde(default)]
    pub one_shot_agent: Option<String>,
    /// Optional family context for one-shot executions.
    #[serde(default)]
    pub one_shot_family_id: Option<String>,
    /// Optional prompt prefix appended to every one-shot request.
    #[serde(default)]
    pub one_shot_prompt: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
