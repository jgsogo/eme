use crate::alexa_speak::{ alexa_speak};
use agent_runtime::Tool;
use async_trait::async_trait;
use serde_json::Value;
pub struct AlexaTool;

#[async_trait]
impl Tool for AlexaTool {
    async fn execute(&self, arguments: Value) -> Result<String, String> {
        let text = arguments
            .get("text")
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .ok_or_else(|| "Missing or invalid 'text' argument".to_owned())?;

        let device_name = std::env::var("AMAZON_ALEXA_DEVICE_NAME")
            .map_err(|_| "AMAZON_ALEXA_DEVICE_NAME env var not set".to_owned())?;

                    alexa_speak(&device_name, text)
            .await
            .map_err(|error| error.to_string())?;

        Ok(format!("Message sent to Alexa device '{}'.", device_name))
    }
}
