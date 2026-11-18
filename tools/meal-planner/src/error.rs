//! Stable errors returned by Meal Planner application and persistence operations.
use thiserror::Error;

/// Errors exposed at the Meal Planner application boundary.
#[derive(Debug, Error)]
pub enum MealPlannerError {
    #[error("invalid meal planner input: {0}")]
    InvalidInput(String),
    #[error("invalid meal planner configuration: {0}")]
    InvalidConfiguration(String),
    #[error("meal planner resource not found")]
    NotFound,
    #[error("meal planner conflict: {0}")]
    Conflict(String),
    #[error("meal planner database schema version {current} is newer than supported version {supported}")]
    UnsupportedSchemaVersion { current: i64, supported: i64 },
    #[error("meal planner storage error: {0}")]
    Storage(#[from] rusqlite::Error),
    #[error("meal planner serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("meal planner filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("meal planner database lock is poisoned")]
    LockPoisoned,
}
