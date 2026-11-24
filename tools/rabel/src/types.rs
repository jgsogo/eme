//! Compact domain types returned by the RABEL adapter.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogueSearchResult {
    /// Opaque record reference returned by RABEL. It is session-scoped; pass it
    /// unchanged to rabel.get_item rather than constructing an OPAC URL.
    pub record_ref: String,
    pub title: String,
    pub authors: Vec<String>,
    pub publication: Option<String>,
    pub isbn: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogueItem {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub publisher: Option<String>,
    pub publication_year: Option<u16>,
    pub isbn: Option<String>,
    pub holdings: Vec<CopyHolding>,
    pub checked_at: DateTime<Utc>,
    pub source_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopyHolding {
    pub library: Option<String>,
    pub signature: Option<String>,
    pub location: Option<String>,
    pub material: Option<String>,
    pub availability: Availability,
    /// Compact text from the catalogue's copy row, retained when the OPAC
    /// exposes fields whose labels vary between installations.
    pub catalogue_note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Available,
    Loaned,
    ReadingRoomOnly,
    Other,
    Unknown,
}
