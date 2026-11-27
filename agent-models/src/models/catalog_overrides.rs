use std::collections::BTreeMap;

use crate::models::{ModelOverride, ProviderOverride};

/// Administrative overrides layered on top of the static provider/model catalogue.
///
/// Keys are provider names and provider/model name pairs respectively. This type
/// contains only sparse changes; it is not a second copy of the TOML catalogue.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelCatalogOverrides {
    pub providers: BTreeMap<String, ProviderOverride>,
    pub models: BTreeMap<(String, String), ModelOverride>,
}
