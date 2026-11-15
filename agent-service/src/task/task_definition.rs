use chrono::{DateTime, FixedOffset};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TaskDefinition {
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
    #[serde(default = "default_enabled")]
    pub(super) enabled: bool,
}

fn default_enabled() -> bool {
    true
}
