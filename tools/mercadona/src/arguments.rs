//! Shared validation and configuration helpers for Mercadona tool arguments.

use agent_runtime::ToolError;
use serde_json::Value;

/// Read a required, non-empty string argument.
pub fn required_string<'a>(arguments: &'a Value, name: &str) -> Result<&'a str, ToolError> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ToolError::Execution(format!("'{name}' must be a non-empty string")))
}

/// Resolve the warehouse from an optional argument or trusted process configuration.
pub fn warehouse(arguments: &Value) -> String {
    arguments
        .get("warehouse")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .or_else(|| std::env::var("MERCADONA_WAREHOUSE").ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "mad1".to_owned())
}
