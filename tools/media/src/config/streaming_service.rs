//! Configurable household streaming subscriptions.
use serde::{Deserialize, Serialize};

/// Streaming services that may be configured as household subscriptions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamingService {
    Netflix,
    PrimeVideo,
    DisneyPlus,
    HboMax,
}
