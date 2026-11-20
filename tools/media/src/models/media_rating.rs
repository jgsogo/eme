//! A source-attributed rating for a media item.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{MediaItemId, MediaProfileId, WatchHistorySource};

/// Rating recorded by a profile, normalized to a five-point scale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaRating {
    pub profile_id: MediaProfileId,
    pub media_item_id: MediaItemId,
    pub value: f32,
    pub rated_at: DateTime<Utc>,
    pub source: WatchHistorySource,
}
