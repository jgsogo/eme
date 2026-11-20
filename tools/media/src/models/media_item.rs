//! Normalized catalogue item; provider response structures stay in adapters.
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use super::{ExternalId, MediaItemId, MediaKind};

/// Provider-independent movie or TV catalogue entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaItem {
    pub id: MediaItemId,
    pub kind: MediaKind,
    pub title: String,
    pub original_title: Option<String>,
    pub overview: Option<String>,
    pub release_date: Option<NaiveDate>,
    pub original_language: Option<String>,
    pub genres: Vec<String>,
    pub runtime_minutes: Option<u16>,
    pub external_ids: Vec<ExternalId>,
}
