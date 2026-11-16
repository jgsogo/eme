//! Errors returned by provider clients and model resolution.

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("Model name is invalid: {0}")]
    InvalidModelName(String),

    #[error("Missing API key: {0}")]
    MissingApiKey(String),

    #[error("Provider '{0}' is not supported")]
    UnsupportedProvider(String),

    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Provider returned an error: {0}")]
    Provider(String),

    #[error("Invalid provider response: {0}")]
    InvalidResponse(String),

    #[error("Unsupported model '{0}'")]
    UnsupportedModel(String),
}
