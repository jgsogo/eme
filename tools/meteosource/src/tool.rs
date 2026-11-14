use agent_runtime::{Tool, ToolError};
use async_trait::async_trait;
use serde_json::Value;

use crate::weather::MeteosourceClient;

/// Retrieves current weather and the hourly forecast for a location.
pub struct MeteosourceWeather;

#[async_trait]
impl Tool for MeteosourceWeather {
    async fn execute(&self, arguments: Value) -> Result<String, ToolError> {
        let location = arguments
            .get("location")
            .and_then(Value::as_str)
            .filter(|location| !location.trim().is_empty())
            .ok_or_else(|| ToolError::InvalidArguments {
                tool_name: "meteosource".into(),
                errors: vec!["Missing or invalid 'location' argument".into()],
            })?;

        let api_key = std::env::var("METEOSOURCE_APIKEY").map_err(|_| {
            ToolError::Configuration("METEOSOURCE_APIKEY env var not set".into())
        })?;

        let client = MeteosourceClient::new(api_key);
        let weather = client
            .weather_for_location(location)
            .await
            .map_err(|error| ToolError::Execution(error.to_string()))?;

        serde_json::to_string_pretty(&weather)
            .map_err(|error| ToolError::Execution(error.to_string()))
    }
}
