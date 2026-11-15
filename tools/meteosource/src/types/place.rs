use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct Place {
    pub(crate) name: String,
    pub(crate) place_id: String,
    #[serde(default)]
    pub(crate) country: Option<String>,
    #[serde(default)]
    pub(crate) timezone: Option<String>,
}
