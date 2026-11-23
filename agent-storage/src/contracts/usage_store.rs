//! Durable persistence contract for invocation usage and immutable pricing.

use super::StorageError;
use agent_metering::{ModelPricing, UsageRecord};
use async_trait::async_trait;
use uuid::Uuid;

/// Stores accounting records independently of transient run events and telemetry.
///
/// Usage records are append-only and keyed by invocation ID. Re-appending an
/// identical record is idempotent; reusing an ID for different data is rejected.
/// Pricing versions are immutable once inserted.
#[async_trait]
pub trait UsageStore: Send + Sync {
    async fn append_usage(&self, record: UsageRecord) -> Result<(), StorageError>;
    async fn list_usage(&self, run_id: Option<Uuid>) -> Result<Vec<UsageRecord>, StorageError>;
    async fn insert_pricing(&self, pricing: ModelPricing) -> Result<(), StorageError>;
    async fn get_pricing(
        &self,
        provider: &str,
        model: &str,
        version: &str,
    ) -> Result<Option<ModelPricing>, StorageError>;
    async fn list_pricing(
        &self,
        provider: &str,
        model: &str,
    ) -> Result<Vec<ModelPricing>, StorageError>;
    async fn list_all_pricing(&self) -> Result<Vec<ModelPricing>, StorageError>;
}
