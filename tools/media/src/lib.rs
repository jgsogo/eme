//! Media discovery and recommendation domain for EME.
//!
//! This crate owns provider-independent media concepts, family media profiles,
//! preferences, viewing activity and recommendation context. Provider adapters,
//! persistence and agent-tool integration are layered on top of these types.
//!
//! EME family membership and authorization remain owned by `agent-storage`
//! and the host application. A media profile belongs to a family but does not
//! require an EME user account.
mod config;
mod models;
mod providers;
mod services;
mod store;
mod tools;

pub use config::{MediaConfig, MediaConfigError, StreamingService};
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
