use super::*;

#[derive(Debug, Deserialize)]
struct Calendar {
    #[serde(rename = "timeZone")]
    pub(super) time_zone: String,
}
