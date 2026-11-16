use crate::alexa_speak::alexa_speak;
use agent_runtime::{Tool, ToolContext, ToolError};
use async_trait::async_trait;
use serde_json::Value;

const SUPPORTED_CHIMES: &[&str] = &[
    "soundbank://soundlibrary/computers/beeps_tones/beeps_tones_06",
    "soundbank://soundlibrary/aircrafts/futuristic/futuristic_01",
    "soundbank://soundlibrary/air/steam/steam_05",
    "soundbank://soundlibrary/water/nature/nature_08",
    "soundbank://soundlibrary/animals/amzn_sfx_bird_chickadee_chirp_1x_01",
    "soundbank://soundlibrary/animals/amzn_sfx_bird_forest_01",
    "soundbank://soundlibrary/animals/amzn_sfx_bird_robin_chirp_1x_01",
    "soundbank://soundlibrary/animals/amzn_sfx_raven_caw_1x_01",
];

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
            if !SUPPORTED_CHIMES.contains(&uri) {
                return Err(ToolError::InvalidArguments {
                    tool_name: "alexa_speak".into(),
                    errors: vec![format!(
                        "Unsupported 'chime'. Choose one of: {}",
                        SUPPORTED_CHIMES.join(", ")
                    )],
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
