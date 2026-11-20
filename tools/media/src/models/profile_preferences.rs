//! Explicit preferences configured for one media profile.
use serde::{Deserialize, Serialize};

use super::MediaProfileId;

/// User-maintained preferences; synchronized viewing activity is not stored here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfilePreferences {
    pub profile_id: MediaProfileId,
    pub liked_genres: Vec<String>,
    pub disliked_genres: Vec<String>,
    pub preferred_languages: Vec<String>,
    pub liked_themes: Vec<String>,
    pub disliked_themes: Vec<String>,
}

impl ProfilePreferences {
    pub fn new(profile_id: MediaProfileId) -> Self {
        Self {
            profile_id,
            liked_genres: Vec::new(),
            disliked_genres: Vec::new(),
            preferred_languages: Vec::new(),
            liked_themes: Vec::new(),
            disliked_themes: Vec::new(),
        }
    }
}
