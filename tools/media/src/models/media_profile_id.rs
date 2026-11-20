//! Stable identity for a family-managed media profile.
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Identifier for a media profile, independent of EME user identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MediaProfileId(Uuid);

impl MediaProfileId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for MediaProfileId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for MediaProfileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
