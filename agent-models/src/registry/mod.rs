//! Loads provider configuration and resolves model identifiers to clients.

#[path = "registry.rs"]
mod model_registry;
mod provider_config;

pub use model_registry::{ProviderInfo, ProviderModelInfo, Registry};
