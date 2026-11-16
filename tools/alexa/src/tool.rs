use crate::alexa_speak::alexa_speak;
use agent_runtime::{Tool, ToolContext, ToolError};
use async_trait::async_trait;
use serde_json::Value;

pub struct AlexaSpeak;

#[async_trait]
impl Tool for AlexaSpeak {
    async fn execute(&self, _context: &ToolContext, arguments: Value) -> Result<String, ToolError> {
        let text = arguments
            .get("text")
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .ok_or_else(|| ToolError::InvalidArguments {
                tool_name: "alexa_speak".into(),
                errors: vec!["Missing or invalid 'text' argument".into()],
            })?;

        let chime = arguments.get("chime").and_then(Value::as_str);
        if let Some(uri) = chime {
            let valid = uri
                .strip_prefix("soundbank://soundlibrary/")
                .is_some_and(|path| {
                    !path.is_empty()
                        && path
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || b"/_-".contains(&byte))
                });
            if !valid {
                return Err(ToolError::InvalidArguments {
                    tool_name: "alexa_speak".into(),
                    errors: vec![
                        "'chime' must be a soundbank://soundlibrary/ URI containing only letters, numbers, '/', '_' or '-'".into(),
                    ],
                });
            }
        }

        let device_name = std::env::var("AMAZON_ALEXA_DEVICE_NAME").map_err(|_| {
            ToolError::Configuration("AMAZON_ALEXA_DEVICE_NAME env var not set".into())
        })?;

        alexa_speak(&device_name, text, chime)
            .await
            .map_err(|error| ToolError::Execution(error.to_string()))?;

        Ok(format!("Message sent to Alexa device '{}'.", device_name))
    }
}
