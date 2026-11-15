use reqwest::StatusCode;
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum GoogleCalendarError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Google Calendar returned HTTP {status}: {detail}")]
    Api { status: StatusCode, detail: String },
    #[error("Google OAuth token refresh failed: {0}")]
    OAuth(String),
    #[error("Invalid date or time value: {0}")]
    InvalidTime(String),
    #[error("Invalid response from Google: {0}")]
    Json(#[from] serde_json::Error),
}
