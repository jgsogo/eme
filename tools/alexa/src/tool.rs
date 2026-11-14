use crate::alexa_speak::alexa_speak;
use agent_runtime::{Tool, ToolError};
use async_trait::async_trait;
use serde_json::Value;

pub struct AlexaSpeak;

#[async_trait]
impl Tool for AlexaSpeak {
    async fn execute(&self, arguments: Value) -> Result<String, ToolError> {
        let text = arguments
            .get("text")
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .ok_or_else(|| ToolError::InvalidArguments {
                tool_name: "alexa_speak".into(),
                errors: vec!["Missing or invalid 'text' argument".into()],
            })?;

        let device_name = std::env::var("AMAZON_ALEXA_DEVICE_NAME").map_err(|_| {
            ToolError::Configuration("AMAZON_ALEXA_DEVICE_NAME env var not set".into())
        })?;

        alexa_speak(&device_name, text)
            .await
            .map_err(|error| ToolError::Execution(error.to_string()))?;

        Ok(format!("Message sent to Alexa device '{}'.", device_name))
    }
}
