use super::super::{default_max_results, default_search_depth};
use super::{SearchDepth, SearchTopic, TimeRange};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct Arguments {
    pub(crate) query: String,
    #[serde(default = "default_max_results")]
    pub(crate) max_results: u8,
    #[serde(default = "default_search_depth")]
    pub(crate) search_depth: SearchDepth,
    #[serde(default)]
    pub(crate) topic: SearchTopic,
    #[serde(default)]
    pub(crate) time_range: Option<TimeRange>,
    #[serde(default)]
    pub(crate) include_domains: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) exclude_domains: Option<Vec<String>>,
}
