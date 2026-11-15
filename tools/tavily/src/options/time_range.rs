use super::super::*;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum TimeRange {
    Day,
    Week,
    Month,
    Year,
}
