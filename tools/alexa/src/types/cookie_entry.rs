use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct CookieEntry {
    #[serde(rename = "Name")]
    pub(crate) name: String,
    #[serde(rename = "Value")]
    pub(crate) value: String,
}
