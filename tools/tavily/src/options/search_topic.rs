use super::super::*;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum SearchTopic {
    #[default]
    General,
    News,
    Finance,
}
