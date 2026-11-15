use super::*;

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct SearchResponse {
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    #[serde(default)]
    pub results: Vec<SearchResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_time: Option<f64>,
}
