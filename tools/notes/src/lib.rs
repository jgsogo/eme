//! User-owned notes with optional sharing to one family.
//!
//! This crate provides persistent note storage and agent tools for listing and
//! retrieving notes. Notes are private by default and may be shared with one
//! family, with access checked against the user's current family memberships.
//!
//! The host application is responsible for exposing note-management interfaces.
//! This crate owns the data model, SQLite schema, access rules, and tool adapters.

mod new_note;
mod note;
mod note_summary;
mod notes_error;
mod store;
mod tools;
mod update_note;

pub use new_note::NewNote;
pub use note::Note;
pub use note_summary::NoteSummary;
pub use notes_error::NotesError;
pub use store::NotesStore;
pub use tools::register_tools;
pub use update_note::UpdateNote;
