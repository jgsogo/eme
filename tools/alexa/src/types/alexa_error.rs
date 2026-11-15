use super::*;

#[derive(Error, Debug)]
pub(super) pub(super) enum AlexaError {
    #[error("AMAZON_ALEXA_REFRESH_TOKEN env var not set")]
    MissingToken,
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Auth failed: {0}")]
    Auth(String),
    #[error("Device '{0}' not found")]
    DeviceNotFound(String),
    #[error("API error: {0}")]
    Api(String),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
}
