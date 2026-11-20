//! External catalogue and availability providers.
mod tmdb;
mod trakt_oauth;
mod trakt_sync;
mod watchmode;

pub use tmdb::{TmdbClient, TmdbError, TmdbPage};
pub use trakt_oauth::{
    TraktAuthorization, TraktError, TraktOAuthConfig, TraktOAuthService, TraktTokenManager,
};
pub use trakt_sync::{TraktSyncError, TraktSyncService, TraktSyncSummary};
pub use watchmode::{WatchmodeClient, WatchmodeError, WatchmodeSource, filter_subscription_offers};
