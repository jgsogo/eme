//! Google integrations for the agent platform.
//!
//! This crate groups tools for Google services. Calendar tools are available
//! today; Gmail tools can be added here without introducing another crate.

pub(crate) mod calendar;
mod calendar_list;
mod google_calendar_availability_tool;
mod google_calendar_list_tool;
mod google_calendar_write_tools;
pub mod oauth;
mod runtime;
mod token_manager;

pub use calendar_list::{GoogleCalendarSummary, list_calendars, list_calendars_with_access_token};
pub use oauth::{
    CalendarScope, GoogleOAuthConfig, GoogleOAuthError, GoogleTokenSet, GoogleUserInfo,
    PendingAuthorization, authorization_request, exchange_code, fetch_user_info,
    refresh_access_token, revoke_token,
};
pub use runtime::GoogleCalendarRuntimeConfig;
pub use token_manager::GoogleOAuthTokenManager;

use std::path::PathBuf;

use agent_config::ConfigError;
use agent_runtime::ToolRegistry;

/// Register all Google tools.
pub fn register_tools(
    registry: &mut ToolRegistry,
    runtime: Option<GoogleCalendarRuntimeConfig>,
) -> Result<(), ConfigError> {
    let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data");
    registry.register_from_file(
        data_dir.join("calendar_list.toml"),
        google_calendar_list_tool::GoogleCalendarList::new(runtime.clone()),
    )?;
    registry.register_from_file(
        data_dir.join("calendar_events.toml"),
        calendar::GoogleCalendarEvents::new(runtime.clone()),
    )?;
    registry.register_from_file(
        data_dir.join("calendar_check_availability.toml"),
        google_calendar_availability_tool::GoogleCalendarCheckAvailability::new(runtime.clone()),
    )?;
    registry.register_from_file(
        data_dir.join("calendar_create_event.toml"),
        google_calendar_write_tools::GoogleCalendarCreateEvent::new(runtime.clone()),
    )?;
    registry.register_from_file(
        data_dir.join("calendar_update_event.toml"),
        google_calendar_write_tools::GoogleCalendarUpdateEvent::new(runtime),
    )?;

    Ok(())
}
