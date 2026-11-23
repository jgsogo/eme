//! Configuration defaults and validation tests.
use super::{MediaConfig, MediaConfigError};

#[test]
fn defaults_target_spain_and_configured_services() {
    let config = MediaConfig::default();
    assert_eq!(config.country, "ES");
    assert_eq!(config.language, "es-ES");
    assert_eq!(config.recommendation_limit, 5);
    assert!(config.require_subscription);
    assert_eq!(config.availability_cache_ttl_seconds, 21_600);
    assert_eq!(config.subscriptions.len(), 4);
    assert!(config.validate().is_ok());
}

#[test]
fn rejects_invalid_country_and_zero_limit() {
    let mut config = MediaConfig::default();
    config.country = "esp".into();
    assert!(matches!(
        config.validate(),
        Err(MediaConfigError::InvalidCountry(_))
    ));

    config.country = "ES".into();
    config.recommendation_limit = 0;
    assert_eq!(
        config.validate(),
        Err(MediaConfigError::ZeroRecommendationLimit)
    );

    config.recommendation_limit = 5;
    config.availability_cache_ttl_seconds = 0;
    assert_eq!(
        config.validate(),
        Err(MediaConfigError::ZeroAvailabilityCacheTtl)
    );
}

#[test]
fn validates_recommendation_weights() {
    let mut config = MediaConfig::default();
    config.recommendation_weights.family_genre_preference = -1;
    assert_eq!(
        config.validate(),
        Err(MediaConfigError::InvalidRecommendationWeights)
    );

    config.recommendation_weights.family_genre_preference = 100;
    config.recommendation_weights.recent_release_years = 5;
    config.recommendation_weights.older_release_years = 2;
    assert_eq!(
        config.validate(),
        Err(MediaConfigError::InvalidRecommendationWeights)
    );
}
