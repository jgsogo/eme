//! Compact domain types returned by the RABEL adapter.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    pub id: String,
    /// Display label with the OPAC's leading hierarchy markers removed.
    pub name: String,
    /// Nesting level encoded by leading '>' characters in the RABEL selector.
    #[serde(default)]
    pub depth: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogueSearchResult {
    /// Opaque record reference returned by RABEL. It is session-scoped; pass it
    /// unchanged to rabel.get_item rather than constructing an OPAC URL.
    pub record_ref: String,
    /// Parsed title proper, without author prefix or statement of responsibility.
    pub title: String,
    pub authors: Vec<String>,
    /// Original publication statement exposed in the search result.
    pub publication: Option<String>,
    pub publisher: Option<String>,
    pub publication_year: Option<u16>,
    pub responsibility: Option<String>,
    pub publication_statement: Option<String>,
    pub physical_description: Option<String>,
    pub series: Option<String>,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub subjects: Vec<String>,
    /// Catalogue document class (for example monograph, video recording, or periodical).
    #[serde(default)]
    pub category: Option<String>,
    pub isbn: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogueItem {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub publisher: Option<String>,
    pub publication_year: Option<u16>,
    pub isbn: Option<String>,
    /// Text following the title proper after RABEL's title/responsibility slash.
    pub responsibility: Option<String>,
    /// Full publication statement as supplied by RABEL.
    pub publication_statement: Option<String>,
    pub physical_description: Option<String>,
    pub series: Option<String>,
    pub notes: Vec<String>,
    pub subjects: Vec<String>,
}

/// Copy-level availability is deliberately separate from bibliographic data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogueHoldings {
    pub copies: Vec<CopyHolding>,
    pub checked_at: DateTime<Utc>,
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
