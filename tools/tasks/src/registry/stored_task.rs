//! On-disk task representation, including fields migrated from older files.

use chrono::{DateTime, FixedOffset};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StoredTask {
    #[serde(default)]
    pub(super) id: Option<Uuid>,
    #[serde(default)]
    pub(super) user_id: Option<String>,
    pub(super) name: String,
    #[serde(default)]
    pub(super) description: String,
    pub(super) agent: String,
    pub(super) prompt: String,
    #[serde(default)]
    pub(super) schedule: Option<String>,
    #[serde(default)]
    pub(super) at: Option<DateTime<FixedOffset>>,
    #[serde(default)]
    pub(super) timezone: Option<String>,
    #[serde(default = "super::default_enabled")]
    pub(super) enabled: bool,
}
