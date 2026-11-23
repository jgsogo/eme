//! Provider-independent domain models used throughout the media feature.
mod content_restrictions;
mod external_id;
mod family_media_preferences;
mod history_completeness;
mod media_item;
mod media_item_id;
mod media_kind;
mod media_profile;
mod media_profile_id;
mod media_rating;
mod media_watch_event;
mod media_watchlist_item;
mod offer_type;
mod profile_preferences;
mod provider_sync_state;
mod recommendation_context;
mod recommendation_criteria;
mod streaming_offer;
mod watch_history_source;
mod watched_policy;

#[cfg(test)]
mod tests;

pub use content_restrictions::ContentRestrictions;
pub use external_id::ExternalId;
pub use family_media_preferences::FamilyMediaPreferences;
pub use history_completeness::HistoryCompleteness;
pub use media_item::MediaItem;
pub use media_item_id::MediaItemId;
pub use media_kind::MediaKind;
pub use media_profile::MediaProfile;
pub use media_profile_id::MediaProfileId;
pub use media_rating::MediaRating;
pub use media_watch_event::MediaWatchEvent;
pub use media_watchlist_item::MediaWatchlistItem;
pub use offer_type::OfferType;
pub use profile_preferences::ProfilePreferences;
pub use provider_sync_state::ProviderSyncState;
pub use recommendation_context::RecommendationContext;
pub use recommendation_criteria::RecommendationCriteria;
pub use streaming_offer::StreamingOffer;
pub use watch_history_source::WatchHistorySource;
pub use watched_policy::WatchedPolicy;
