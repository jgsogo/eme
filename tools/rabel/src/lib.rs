//! RABEL / AbsysNet catalogue integration for the Salamanca Public Library.
//!
//! The client keeps AbsysNet's session and cookie handling behind a small API.
//! Agent-facing tools return compact structured data rather than catalogue HTML.

mod client;
mod parser;
mod tool;
mod types;

use std::{path::PathBuf, sync::Arc};

use agent_config::ConfigError;
use agent_runtime::ToolRegistry;

pub use client::RabelClient;
pub use types::{Availability, CatalogueItem, CatalogueSearchResult, CopyHolding, Library};

/// Register catalogue search, record-detail and library-list tools.
pub fn register_tools(registry: &mut ToolRegistry) -> Result<(), ConfigError> {
    let client = Arc::new(RabelClient::new().map_err(|error| {
        ConfigError::invalid("tool", None, format!("cannot create RABEL client: {error}"))
    })?);
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data");
    registry.register_from_file(data.join("search_catalog.toml"), tool::SearchCatalog(client.clone()))?;
    registry.register_from_file(data.join("get_item.toml"), tool::GetItem(client.clone()))?;
    registry.register_from_file(data.join("list_libraries.toml"), tool::ListLibraries(client))?;
    Ok(())
}
