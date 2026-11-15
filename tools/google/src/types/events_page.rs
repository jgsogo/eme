use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub(crate) struct EventsPage {
    #[serde(default)]
    pub(crate) items: Vec<Value>,
    #[serde(rename = "nextPageToken")]
    pub(crate) next_page_token: Option<String>,
}
