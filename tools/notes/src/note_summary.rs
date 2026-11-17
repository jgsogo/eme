//! Metadata returned when listing accessible notes.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::Note;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteSummary {
    pub id: String,
    pub owner_id: String,
    pub family_id: Option<String>,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Note> for NoteSummary {
    fn from(note: Note) -> Self {
        Self {
            id: note.id,
            owner_id: note.owner_id,
            family_id: note.family_id,
            title: note.title,
            created_at: note.created_at,
            updated_at: note.updated_at,
        }
    }
}
