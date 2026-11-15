use super::*;

#[derive(Debug, Deserialize)]
struct CookieEntry {
    #[serde(rename = "Name")]
    pub(super) name: String,
    #[serde(rename = "Value")]
    pub(super) value: String,
}
