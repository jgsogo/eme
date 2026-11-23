//! A profile's saved-to-watchlist media item.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{MediaItemId, MediaProfileId, WatchHistorySource};

/// A watchlist entry with provider provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaWatchlistItem {
    pub profile_id: MediaProfileId,
    pub media_item_id: MediaItemId,
    pub added_at: DateTime<Utc>,
    pub source: WatchHistorySource,
}
