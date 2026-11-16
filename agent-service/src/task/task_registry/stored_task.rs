//! On-disk task representation, including fields migrated from older files.

use chrono::{DateTime, FixedOffset};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StoredTask {
    #[serde(default)]
    id: Option<Uuid>,
    #[serde(default)]
    user_id: Option<String>,
    name: String,
    #[serde(default)]
    description: String,
    agent: String,
    prompt: String,
    #[serde(default)]
    schedule: Option<String>,
    #[serde(default)]
    at: Option<DateTime<FixedOffset>>,
    #[serde(default)]
    timezone: Option<String>,
    #[serde(default = "super::default_enabled")]
    enabled: bool,
}
