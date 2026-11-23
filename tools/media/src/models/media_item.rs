//! Normalized catalogue item; provider response structures stay in adapters.
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use super::{ExternalId, MediaItemId, MediaKind};

/// Provider-independent movie or TV catalogue entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// Regional content certification when available (e.g. Spain's `12` or `18`).
    pub certification: Option<String>,
    /// Provider popularity signal, retained for deterministic ranking.
    pub popularity: Option<f32>,
    pub vote_average: Option<f32>,
    pub vote_count: Option<u32>,
    pub external_ids: Vec<ExternalId>,
}
