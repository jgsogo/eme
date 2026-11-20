//! Runtime configuration types for media discovery.
mod media_config;
mod media_config_error;
mod streaming_service;
#[cfg(test)]
mod tests;

pub use media_config::MediaConfig;
pub use media_config_error::MediaConfigError;
pub use streaming_service::StreamingService;
