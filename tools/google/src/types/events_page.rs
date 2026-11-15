use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub(super) struct EventsPage {
    #[serde(default)]
    pub(super) items: Vec<Value>,
    #[serde(rename = "nextPageToken")]
    pub(super) next_page_token: Option<String>,
}
