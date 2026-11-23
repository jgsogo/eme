//! User-specified constraints and preferences for one recommendation request.
use serde::{Deserialize, Serialize};

use super::MediaKind;

/// Optional filters supplied for a recommendation request.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RecommendationCriteria {
    pub kinds: Vec<MediaKind>,
    pub genres: Vec<String>,
    /// Free-text themes matched against normalized title/overview metadata.
    pub themes: Vec<String>,
    pub languages: Vec<String>,
    pub maximum_runtime_minutes: Option<u16>,
    pub query: Option<String>,
}
