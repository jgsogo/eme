//! Generic tools that expose information available from the local runtime.
mod datetime;
mod sleep;
use std::path::PathBuf;
use agent_config::ConfigError;
use agent_runtime::ToolRegistry;
/// Register all generic tools.
pub fn register_tools(registry: &mut ToolRegistry) -> Result<(), ConfigError> {
 let data=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data");
 registry.register_from_file(data.join("current_datetime.toml"),datetime::CurrentDateTime)?;
 registry.register_from_file(data.join("get_timezone.toml"),datetime::GetTimezone)?;
 registry.register_from_file(data.join("convert_timezone.toml"),datetime::ConvertTimezone)?;
 registry.register_from_file(data.join("calculate_date_difference.toml"),datetime::CalculateDateDifference)?;
 registry.register_from_file(data.join("get_date_info.toml"),datetime::GetDateInfo)?;
 registry.register_from_file(data.join("parse_datetime.toml"),datetime::ParseDateTime)?;
 registry.register_from_file(data.join("sleep.toml"),sleep::Sleep)
}
