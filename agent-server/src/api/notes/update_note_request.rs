//! JSON payload for updating a note.

use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct UpdateNoteRequest {
    pub title: String,
    pub content: String,
    pub family_id: Option<String>,
}
