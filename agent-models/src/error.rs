#[derive(Debug, thiserror::Error)]
pub enum ModelError {

    #[error("Model name is invalid: {0}")]
    InvalidModelName(String),

    #[error("Missing API key: {0}")]
    MissingApiKey(String),

    #[error("Provider '{0}' is not supported")]
    UnsupportedProvider(String)
}