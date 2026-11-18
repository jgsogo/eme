//! Stable errors returned by Meal Planner application and persistence operations.

use thiserror::Error;

/// Errors exposed at the Meal Planner application boundary.
#[derive(Debug, Error)]
pub enum MealPlannerError {
    /// Input failed deterministic application validation.
    #[error("invalid meal planner input: {0}")]
    InvalidInput(String),

    /// Configuration is missing or invalid.
    #[error("invalid meal planner configuration: {0}")]
    InvalidConfiguration(String),

    /// The requested domain resource does not exist or is not visible.
    #[error("meal planner resource not found")]
    NotFound,

    /// The requested operation conflicts with the current domain state.
    #[error("meal planner conflict: {0}")]
    Conflict(String),

    /// The database is newer than this version of the application supports.
    #[error(
        "meal planner database schema version {current} is newer than supported version {supported}"
    )]
    UnsupportedSchemaVersion { current: i64, supported: i64 },

    /// SQLite returned a persistence error.
    #[error("meal planner storage error: {0}")]
    Storage(#[from] rusqlite::Error),

    /// Creating the database directory failed.
    #[error("meal planner filesystem error: {0}")]
    Io(#[from] std::io::Error),

    /// A database lock was poisoned by a previous panic.
    #[error("meal planner database lock is poisoned")]
    LockPoisoned,
}
