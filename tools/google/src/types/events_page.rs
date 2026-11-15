use super::*;

#[derive(Debug, Deserialize)]
struct EventsPage {
    #[serde(default)]
    pub(super) items: Vec<Value>,
    #[serde(rename = "nextPageToken")]
    pub(super) next_page_token: Option<String>,
}
