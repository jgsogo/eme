//! Stable identity for a normalized media catalogue item.
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// EME-owned identifier for a media item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MediaItemId(Uuid);

impl MediaItemId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for MediaItemId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for MediaItemId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
