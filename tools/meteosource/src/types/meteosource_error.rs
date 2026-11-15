use reqwest::StatusCode;
use thiserror::Error;

#[derive(Debug, Error)]
pub(super) enum MeteosourceError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Meteosource returned HTTP {status}: {detail}")]
    Api { status: StatusCode, detail: String },

    #[error("No location matching '{0}' was found")]
    PlaceNotFound(String),

    #[error("Invalid response from Meteosource: {0}")]
    Json(#[from] serde_json::Error),
}
