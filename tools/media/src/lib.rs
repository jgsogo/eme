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
mod store;

pub use config::{MediaConfig, MediaConfigError, StreamingService};
pub use store::{MediaStore, MediaStoreError};

pub use models::{
    ContentRestrictions, ExternalId, FamilyMediaPreferences, HistoryCompleteness, MediaItem,
    MediaItemId, MediaKind, MediaProfile, MediaProfileId, MediaRating, MediaWatchEvent, OfferType,
    ProfilePreferences, RecommendationContext, RecommendationCriteria, StreamingOffer,
    WatchHistorySource, WatchedPolicy,
};
