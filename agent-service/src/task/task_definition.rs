use chrono::{DateTime, FixedOffset};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TaskDefinition {
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
    #[serde(default = "default_enabled")]
    enabled: bool,
}

fn default_enabled() -> bool {
    true
}
