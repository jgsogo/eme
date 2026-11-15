use super::super::*;

#[derive(Debug, Deserialize)]
pub(super) struct Calendar {
    #[serde(rename = "timeZone")]
    pub(super) time_zone: String,
}
