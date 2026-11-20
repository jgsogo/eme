//! A recorded viewing event for a profile.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{MediaItemId, MediaProfileId, WatchHistorySource};

/// A source-attributed viewing event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaWatchEvent {
    pub profile_id: MediaProfileId,
    pub media_item_id: MediaItemId,
    pub watched_at: DateTime<Utc>,
    pub source: WatchHistorySource,
}
