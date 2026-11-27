//! Persistence contract for sparse provider/model catalogue overrides.

use agent_models::{ModelCatalogOverrides, ModelOverride, ProviderOverride};

use super::StorageError;

/// Reads and updates administrative overrides without owning the static catalogue.
#[async_trait::async_trait]
pub trait ModelCatalogOverrideStore: Send + Sync {
    async fn load_overrides(&self) -> Result<ModelCatalogOverrides, StorageError>;

    async fn set_provider_override(
        &self,
        provider: &str,
        value: ProviderOverride,
    ) -> Result<(), StorageError>;

    async fn clear_provider_override(&self, provider: &str) -> Result<(), StorageError>;

    async fn set_model_override(
        &self,
        provider: &str,
        model: &str,
        value: ModelOverride,
    ) -> Result<(), StorageError>;

    async fn clear_model_override(
        &self,
        provider: &str,
        model: &str,
    ) -> Result<(), StorageError>;
}
