//! Errors raised while validating media configuration.
use thiserror::Error;

/// Invalid media configuration.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MediaConfigError {
    #[error("country must be a two-letter uppercase ISO 3166-1 code, got '{0}'")]
    InvalidCountry(String),
    #[error("preferred language must not be empty")]
    EmptyLanguage,
    #[error("recommendation limit must be greater than zero")]
    ZeroRecommendationLimit,
    #[error("availability cache TTL must be greater than zero")]
    ZeroAvailabilityCacheTtl,
}
