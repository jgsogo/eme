//! Optional fields used to update an existing note.

#[derive(Debug, Clone, Default)]
pub struct UpdateNote {
    pub title: Option<String>,
    pub content: Option<String>,
    pub family_id: Option<Option<String>>,
}
