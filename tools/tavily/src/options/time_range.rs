use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TimeRange {
    Day,
    Week,
    Month,
    Year,
}
