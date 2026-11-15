use reqwest::StatusCode;
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum TavilyError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Tavily returned HTTP {status}: {detail}")]
    Api { status: StatusCode, detail: String },

    #[error("Invalid response from Tavily: {0}")]
    Json(#[from] serde_json::Error),
}
