//! Runtime errors and their stable classification for callers and telemetry.

mod error_category;
mod runtime_error;

pub use error_category::ErrorCategory;
pub use runtime_error::RuntimeError;
