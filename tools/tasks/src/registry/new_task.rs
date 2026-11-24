//! Input required to create a scheduled or one-time task.

use agent_storage::FamilyId;
use chrono::{DateTime, FixedOffset};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewTask {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub agent: String,
    pub prompt: String,
    #[serde(default)]
    pub schedule: Option<String>,
    #[serde(default)]
    pub at: Option<DateTime<FixedOffset>>,
    #[serde(default)]
    pub timezone: Option<String>,
    /// Optional family whose current context is supplied to scheduled executions.
    #[serde(default)]
    pub family_id: Option<FamilyId>,
}
