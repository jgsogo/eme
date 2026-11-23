//! Incremental synchronization cursor for one profile and provider.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::MediaProfileId;

/// Provider activity cursor and last successful synchronization timestamp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderSyncState {
    pub profile_id: MediaProfileId,
    pub provider: String,
    /// Provider-specific opaque cursor; serialized by the integration adapter.
    pub cursor: Option<String>,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub status: String,
}
