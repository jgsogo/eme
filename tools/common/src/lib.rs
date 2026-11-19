//! Generic tools that expose useful information from the runtime and external public data sources.

mod datetime;
mod holidays;
mod sleep;
mod tweet;

use agent_config::ConfigError;
use agent_runtime::ToolRegistry;
use std::path::PathBuf;

/// Register all generic tools.
pub fn register_tools(registry: &mut ToolRegistry) -> Result<(), ConfigError> {
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data");

    registry.register_from_file(
        data.join("current_datetime.toml"),
        datetime::CurrentDateTime,
    )?;
    registry.register_from_file(data.join("get_timezone.toml"), datetime::GetTimezone)?;
    registry.register_from_file(
        data.join("convert_timezone.toml"),
        datetime::ConvertTimezone,
    )?;
    registry.register_from_file(
        data.join("calculate_date_difference.toml"),
        datetime::CalculateDateDifference,
    )?;
    registry.register_from_file(data.join("get_date_info.toml"), datetime::GetDateInfo)?;
    registry.register_from_file(data.join("parse_datetime.toml"), datetime::ParseDateTime)?;
    registry.register_from_file(data.join("get_holidays.toml"), holidays::GetHolidays)?;
    registry.register_from_file(data.join("get_tweet.toml"), tweet::GetTweet)?;
    registry.register_from_file(data.join("sleep.toml"), sleep::Sleep)
}
