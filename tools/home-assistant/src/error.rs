//! Error types for Home Assistant API requests.

use thiserror::Error;

/// Errors returned by the Home Assistant client.
#[derive(Debug, Error)]
pub enum HomeAssistantError {
    #[error("invalid Home Assistant URL: {0}")]
    InvalidUrl(String),
    #[error("Home Assistant request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Home Assistant returned HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("invalid Home Assistant API response: {0}")]
    Response(#[from] serde_json::Error),
}
