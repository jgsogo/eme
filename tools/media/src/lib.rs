//! Media discovery and recommendation domain for EME.
//!
//! This crate owns provider-independent media concepts, personal and family
//! media profiles, preferences, viewing activity and recommendation context.
//! Provider adapters, persistence and agent-tool integration are layered on top.
//!
//! EME identity and family membership remain owned by `agent-storage` and the
//! host application. Every EME user can have a personal media profile without
//! joining a family; family-managed profiles remain available for household use.
mod config;
mod models;
mod providers;
mod services;
mod store;
mod tools;

pub use config::{MediaConfig, MediaConfigError, RecommendationWeights, StreamingService};
pub use providers::{
    TmdbClient, TmdbError, TmdbPage, TraktAuthorization, TraktError, TraktOAuthConfig,
    TraktOAuthService, TraktSyncError, TraktSyncService, TraktSyncSummary, TraktTokenManager,
    WatchmodeClient, WatchmodeError, WatchmodeSource, filter_subscription_offers,
};
pub use services::{MediaRecommendation, RecommendationError, RecommendationService};
pub use store::{MediaExternalConnectionLink, MediaStore, MediaStoreError, PendingTraktLink};
pub use tools::{MediaToolServices, register_media_tools};

pub use models::{
    ContentRestrictions, ExternalId, FamilyMediaPreferences, HistoryCompleteness, MediaItem,
    MediaItemId, MediaKind, MediaProfile, MediaProfileId, MediaRating, MediaWatchEvent,
    MediaWatchlistItem, OfferType, ProfilePreferences, ProviderSyncState, RecommendationContext,
    RecommendationCriteria, StreamingOffer, WatchHistorySource, WatchedPolicy,
};
