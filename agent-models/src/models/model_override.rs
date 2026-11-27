use crate::models::AccessTier;

/// Sparse administrative overrides for one model.
///
/// A missing field inherits the value from the version-controlled TOML catalogue.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelOverride {
    pub enabled: Option<bool>,
    pub access_tier: Option<AccessTier>,
    pub disabled_reason: Option<String>,
}
