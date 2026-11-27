/// Sparse administrative overrides for one model.
///
/// A missing field inherits the value from the version-controlled TOML catalogue.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelOverride {
    pub enabled: Option<bool>,
    pub disabled_reason: Option<String>,
}
