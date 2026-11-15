use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct Calendar {
    #[serde(rename = "timeZone")]
    pub(super) time_zone: String,
}
