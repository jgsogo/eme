use super::super::*;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum SearchDepth {
    #[default]
    Basic,
    Advanced,
    Fast,
    UltraFast,
}
