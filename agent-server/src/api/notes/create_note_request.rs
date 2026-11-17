//! JSON payload for creating a note.

use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct CreateNoteRequest {
    pub title: String,
    pub content: String,
    pub family_id: Option<String>,
}
