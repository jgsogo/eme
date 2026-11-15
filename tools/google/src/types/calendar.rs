use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct Calendar {
    #[serde(rename = "timeZone")]
    pub(crate) time_zone: String,
}
