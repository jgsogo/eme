//! Loads provider configuration and resolves model identifiers to clients.

mod provider_config;
#[path = "registry.rs"]
mod model_registry;

pub use model_registry::{ProviderInfo, ProviderModelInfo, Registry};
