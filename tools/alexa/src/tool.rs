//! Runtime adapter for the Alexa text-to-speech operation.

use crate::alexa_speak::alexa_speak;
use agent_runtime::{Tool, ToolContext, ToolError};
use async_trait::async_trait;
use serde_json::Value;

fn valid_chime(chime: &str) -> bool {
    chime == "none"
        || chime
            .strip_prefix("soundbank://soundlibrary/")
            .is_some_and(|path| {
                !path.is_empty()
                    && path
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"/_-".contains(&byte))
            })
}

fn parse_volume(value: &Value) -> Option<u8> {
    value
        .as_u64()
        .filter(|volume| *volume <= 100)
        .map(|volume| volume as u8)
}

pub struct AlexaSpeak;

#[async_trait]
impl Tool for AlexaSpeak {
    async fn execute(&self, _context: &ToolContext, arguments: Value) -> Result<String, ToolError> {
        let text = arguments
            .get("text")
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .ok_or_else(|| ToolError::InvalidArguments {
                tool_name: "alexa.speak".into(),
                errors: vec!["Missing or invalid 'text' argument".into()],
            })?;

        let chime = arguments.get("chime").and_then(Value::as_str);
        if let Some(uri) = chime
            && !valid_chime(uri)
        {
            return Err(ToolError::InvalidArguments {
                tool_name: "alexa.speak".into(),
                errors: vec![
                    "'chime' must be 'none' or a soundbank://soundlibrary/ URI containing only letters, numbers, '/', '_' or '-'".into(),
                ],
            });
        }

        let volume = match arguments.get("volume") {
            None => None,
            Some(value) => Some(parse_volume(value).ok_or_else(|| ToolError::InvalidArguments {
                tool_name: "alexa.speak".into(),
                errors: vec![
                    "'volume' must be an integer between 0 and 100 (inclusive)".into(),
                ],
            })?),
        };

        let device_name = std::env::var("AMAZON_ALEXA_DEVICE_NAME").map_err(|_| {
            ToolError::Configuration("AMAZON_ALEXA_DEVICE_NAME env var not set".into())
        })?;

        alexa_speak(&device_name, text, chime, volume)
            .await
            .map_err(|error| ToolError::Execution(error.to_string()))?;

        Ok(format!("Message sent to Alexa device '{}'.", device_name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_volume_boundaries() {
        assert_eq!(parse_volume(&Value::from(0)), Some(0));
        assert_eq!(parse_volume(&Value::from(100)), Some(100));
    }

    #[test]
    fn rejects_volume_outside_range_or_non_integer_values() {
        assert_eq!(parse_volume(&Value::from(101)), None);
        assert_eq!(parse_volume(&Value::from(-1)), None);
        assert_eq!(parse_volume(&Value::from(12.5)), None);
        assert_eq!(parse_volume(&Value::from("50")), None);
    }

    #[test]
    fn accepts_any_sound_library_uri() {
        assert!(valid_chime(
            "soundbank://soundlibrary/alarms/beeps_and_bloops/intro_02"
        ));
        assert!(valid_chime(
            "soundbank://soundlibrary/animals/amzn_sfx_cat_angry_meow_1x_01"
        ));
    }

    #[test]
    fn accepts_none_to_disable_sound() {
        assert!(valid_chime("none"));
    }

    #[test]
    fn rejects_invalid_chime_values() {
        assert!(!valid_chime("https://example.com/sound.mp3"));
        assert!(!valid_chime("soundbank://soundlibrary/"));
        assert!(!valid_chime("soundbank://soundlibrary/foo\"/><speak>"));
    }
}
