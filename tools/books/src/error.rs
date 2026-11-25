use thiserror::Error;

#[derive(Debug, Error)]
pub enum BookStoreError {
    #[error("book was not found")]
    NotFound,
    #[error("invalid book data: {0}")]
    Invalid(String),
    #[error("book storage failed: {0}")]
    Storage(#[from] rusqlite::Error),
    #[error("book data could not be decoded: {0}")]
    Serialization(#[from] serde_json::Error),
}
