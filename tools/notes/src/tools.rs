//! Agent tool adapters for listing and reading accessible notes.

mod get_note;
mod list_notes;

use std::{error::Error, path::Path, sync::Arc};

use agent_runtime::ToolRegistry;
use agent_storage::FamilyStore;

use crate::NotesStore;

pub fn register_tools(
    registry: &mut ToolRegistry,
    families: Arc<dyn FamilyStore>,
    database: impl AsRef<Path>,
) -> Result<NotesStore, Box<dyn Error + Send + Sync>> {
    let store = NotesStore::open(database, families)?;
    registry.register(
        get_note::definition(),
        get_note::GetNote::new(store.clone()),
    )?;
    registry.register(
        list_notes::definition(),
        list_notes::ListNotes::new(store.clone()),
    )?;
    Ok(store)
}
