use super::{SearchDepth, SearchTopic, TimeRange};
use super::super::{default_max_results, default_search_depth};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct Arguments {
    pub(super) query: String,
    #[serde(default = "default_max_results")]
    pub(super) max_results: u8,
    #[serde(default = "default_search_depth")]
    pub(super) search_depth: SearchDepth,
    #[serde(default)]
    pub(super) topic: SearchTopic,
    #[serde(default)]
    pub(super) time_range: Option<TimeRange>,
    #[serde(default)]
    pub(super) include_domains: Option<Vec<String>>,
    #[serde(default)]
    pub(super) exclude_domains: Option<Vec<String>>,
}
