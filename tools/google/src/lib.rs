//! Google integrations for the agent platform.
//!
//! This crate groups tools for Google services. Calendar tools are available
//! today; Gmail tools can be added here without introducing another crate.

mod calendar;
mod calendar_list;
mod runtime;
pub mod oauth;

pub use calendar_list::{GoogleCalendarSummary, list_calendars};
pub use runtime::GoogleCalendarRuntimeConfig;
pub use oauth::{
    CalendarScope, GoogleOAuthConfig, GoogleOAuthError, GoogleTokenSet, GoogleUserInfo,
    PendingAuthorization, authorization_request, exchange_code, fetch_user_info,
    refresh_access_token, revoke_token,
};

use std::{path::PathBuf, sync::Arc};

use agent_config::ConfigError;
use agent_runtime::ToolRegistry;

/// Register all Google tools.
pub fn register_tools(
    registry: &mut ToolRegistry,
    runtime: Option<GoogleCalendarRuntimeConfig>,
) -> Result<(), ConfigError> {
    let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data");
    registry.register_from_file(
        data_dir.join("calendar_events.toml"),
        calendar::GoogleCalendarEvents::new(runtime),
    )?;

    Ok(())
}
