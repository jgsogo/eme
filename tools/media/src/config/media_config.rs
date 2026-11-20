//! Catalogue and recommendation defaults loaded by the host application.
use serde::{Deserialize, Serialize};

use crate::models::WatchedPolicy;

use super::{MediaConfigError, StreamingService};

/// Media feature defaults loaded by the host application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaConfig {
    /// ISO 3166-1 alpha-2 catalogue region.
    pub country: String,
    /// Preferred metadata language (BCP 47).
    pub language: String,
    /// Configured subscriptions, not streaming-platform credentials.
    pub subscriptions: Vec<StreamingService>,
    /// Default maximum number of recommendations returned.
    pub recommendation_limit: usize,
    /// Whether recommendations must be available on a configured subscription.
    pub require_subscription: bool,
    /// Watchmode availability cache lifetime in seconds.
    pub availability_cache_ttl_seconds: u64,
    /// Default policy for excluding watched titles.
    pub watched_policy: WatchedPolicy,
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            country: "ES".into(),
            language: "es-ES".into(),
            subscriptions: vec![
                StreamingService::Netflix,
                StreamingService::PrimeVideo,
                StreamingService::DisneyPlus,
                StreamingService::HboMax,
            ],
            recommendation_limit: 5,
            require_subscription: true,
            availability_cache_ttl_seconds: 6 * 60 * 60,
            watched_policy: WatchedPolicy::AnyParticipant,
        }
    }
}

impl MediaConfig {
    /// Validate values that would otherwise make provider requests ambiguous.
    pub fn validate(&self) -> Result<(), MediaConfigError> {
        if self.country.len() != 2 || !self.country.bytes().all(|byte| byte.is_ascii_uppercase()) {
            return Err(MediaConfigError::InvalidCountry(self.country.clone()));
        }
        if self.language.trim().is_empty() {
            return Err(MediaConfigError::EmptyLanguage);
        }
        if self.recommendation_limit == 0 {
            return Err(MediaConfigError::ZeroRecommendationLimit);
        }
        if self.availability_cache_ttl_seconds == 0 {
            return Err(MediaConfigError::ZeroAvailabilityCacheTtl);
        }
        Ok(())
    }
}
