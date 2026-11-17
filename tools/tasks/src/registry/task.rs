//! A persisted task definition and its computed next execution time.

use chrono::{DateTime, FixedOffset, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub id: Uuid,
    pub user_id: String,
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
    #[serde(default = "super::default_enabled")]
    pub enabled: bool,
    #[serde(skip)]
    pub next_run: Option<DateTime<Utc>>,
}
