//! Identifiers assigned by external media providers.
use serde::{Deserialize, Serialize};

/// External catalogue identifier, namespaced by provider.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExternalId {
    pub provider: String,
    pub value: String,
}
