use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SearchTopic {
    #[default]
    General,
    News,
    Finance,
}
