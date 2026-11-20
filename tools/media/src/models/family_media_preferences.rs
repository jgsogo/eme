//! Explicit preferences shared by a family, independent of individual profiles.
use agent_storage::FamilyId;
use serde::{Deserialize, Serialize};

/// Shared household taste that contributes to every family recommendation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FamilyMediaPreferences {
    pub family_id: FamilyId,
    pub liked_genres: Vec<String>,
    pub disliked_genres: Vec<String>,
    pub preferred_languages: Vec<String>,
    pub liked_themes: Vec<String>,
    pub disliked_themes: Vec<String>,
}

impl FamilyMediaPreferences {
    pub fn new(family_id: FamilyId) -> Self {
        Self {
            family_id,
            liked_genres: Vec::new(),
            disliked_genres: Vec::new(),
            preferred_languages: Vec::new(),
            liked_themes: Vec::new(),
            disliked_themes: Vec::new(),
        }
    }
}

impl Default for FamilyMediaPreferences {
    fn default() -> Self {
        Self::new(FamilyId::new())
    }
}
