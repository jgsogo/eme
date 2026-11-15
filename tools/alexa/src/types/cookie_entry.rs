use super::*;

#[derive(Debug, Deserialize)]
pub(super) struct CookieEntry {
    #[serde(rename = "Name")]
    pub(super) name: String,
    #[serde(rename = "Value")]
    pub(super) value: String,
}
