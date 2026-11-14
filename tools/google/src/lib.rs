//! Google integrations for the agent platform.
//!
//! This crate groups tools for Google services. Calendar tools are available
//! today; Gmail tools can be added here without introducing another crate.

mod calendar;

use std::path::PathBuf;

use agent_config::ConfigError;
use agent_runtime::ToolRegistry;

/// Register all Google tools.
pub fn register_tools(registry: &mut ToolRegistry) -> Result<(), ConfigError> {
    let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data");
    registry.register_from_file(
        data_dir.join("calendar_events.toml"),
        calendar::GoogleCalendarEvents,
    )?;

    Ok(())
}
