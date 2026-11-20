//! Provenance of recorded viewing activity.
use serde::{Deserialize, Serialize};

/// System that supplied a viewing event or rating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchHistorySource {
    Eme,
    Trakt,
}
