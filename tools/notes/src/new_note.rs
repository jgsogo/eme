//! Input used to create a note.

#[derive(Debug, Clone)]
pub struct NewNote {
    pub owner_id: String,
    pub family_id: Option<String>,
    pub title: String,
    pub content: String,
}
