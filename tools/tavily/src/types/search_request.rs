use super::super::*;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SearchRequest {
    pub query: String,
    pub search_depth: String,
    pub topic: String,
    pub max_results: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_range: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude_domains: Option<Vec<String>>,
}
