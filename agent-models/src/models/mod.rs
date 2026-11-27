//! Model metadata: capabilities, access classifications, and catalogue overlays.

mod access_tier;
mod capability;
mod catalog_overrides;
mod config;
mod model_override;
mod provider_override;

pub use access_tier::AccessTier;
pub use capability::ModelCapability;
pub use catalog_overrides::ModelCatalogOverrides;
pub use config::ModelConfig;
pub use model_override::ModelOverride;
pub use provider_override::ProviderOverride;
