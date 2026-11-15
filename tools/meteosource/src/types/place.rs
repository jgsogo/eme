use super::*;

#[derive(Debug, Deserialize)]
struct Place {
    pub(super) name: String,
    pub(super) place_id: String,
    #[serde(default)]
    pub(super) country: Option<String>,
    #[serde(default)]
    pub(super) timezone: Option<String>,
}
