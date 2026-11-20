//! Configurable deterministic recommendation scoring weights.
use serde::{Deserialize, Serialize};

/// Independent weights for shared family preferences and selected profile signals.
///
/// Values are score points (or score multipliers) and default to the current
/// baseline. Set a weight to zero to disable that signal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RecommendationWeights {
    pub audience_rating_scale: i32,
    pub family_genre_preference: i32,
    pub family_genre_dislike_penalty: i32,
    pub family_language_preference: i32,
    pub profile_genre_preference: i32,
    pub profile_genre_dislike_penalty: i32,
    pub profile_language_preference: i32,
    pub rating_affinity_scale: i32,
    pub audience_quality_bonus: i32,
    pub popularity_scale: i32,
    pub recent_release_bonus: i32,
    pub older_release_bonus: i32,
    pub recent_release_years: u8,
    pub older_release_years: u8,
    pub diversity_penalty: i32,
}

impl Default for RecommendationWeights {
    fn default() -> Self {
        Self {
            audience_rating_scale: 100,
            family_genre_preference: 100,
            family_genre_dislike_penalty: 100,
            family_language_preference: 40,
            profile_genre_preference: 60,
            profile_genre_dislike_penalty: 150,
            profile_language_preference: 30,
            rating_affinity_scale: 30,
            audience_quality_bonus: 40,
            popularity_scale: 10,
            recent_release_bonus: 30,
            older_release_bonus: 15,
            recent_release_years: 2,
            older_release_years: 5,
            diversity_penalty: 25,
        }
    }
}

impl RecommendationWeights {
    pub(crate) fn is_valid(&self) -> bool {
        self.audience_rating_scale >= 0
            && self.family_genre_preference >= 0
            && self.family_genre_dislike_penalty >= 0
            && self.family_language_preference >= 0
            && self.profile_genre_preference >= 0
            && self.profile_genre_dislike_penalty >= 0
            && self.profile_language_preference >= 0
            && self.rating_affinity_scale >= 0
            && self.audience_quality_bonus >= 0
            && self.popularity_scale >= 0
            && self.recent_release_bonus >= 0
            && self.older_release_bonus >= 0
            && self.diversity_penalty >= 0
            && self.older_release_years >= self.recent_release_years
    }
}
