//! Hard suitability constraints for an individual participant.
use serde::{Deserialize, Serialize};

use super::MediaProfileId;

/// Restrictions applied before recommendation ranking, not soft preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentRestrictions {
    pub profile_id: MediaProfileId,
    /// Maximum certification accepted for this profile (provider-normalized value).
    pub maximum_certification: Option<String>,
    /// Explicitly prohibited content genres.
    pub disallowed_genres: Vec<String>,
    /// Whether titles without a known certification may pass the restriction check.
    pub allow_unrated: bool,
}

impl ContentRestrictions {
    pub fn new(profile_id: MediaProfileId) -> Self {
        Self {
            profile_id,
            maximum_certification: None,
            disallowed_genres: Vec::new(),
            allow_unrated: false,
        }
    }
}
