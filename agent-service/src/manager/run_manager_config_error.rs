//! Configuration errors returned when creating a run manager.

use thiserror::Error;

/// Configuration errors for a run manager.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RunManagerConfigError {
    #[error("max_concurrent_runs must be greater than zero")]
    ZeroConcurrency,
    #[error("max_queued_runs must be greater than zero")]
    ZeroQueueCapacity,
}
