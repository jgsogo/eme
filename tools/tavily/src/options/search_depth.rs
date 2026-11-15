use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SearchDepth {
    #[default]
    Basic,
    Advanced,
    Fast,
    UltraFast,
}
