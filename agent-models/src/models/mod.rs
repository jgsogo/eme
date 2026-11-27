//! Model metadata: supported capabilities, access classifications, and configuration.

mod access_tier;
mod capability;
mod config;

pub use access_tier::AccessTier;
pub use capability::ModelCapability;
pub use config::ModelConfig;
