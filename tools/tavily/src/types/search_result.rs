use super::super::*;

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct SearchResult {
    pub title: String,
    pub url: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_date: Option<String>,
}
