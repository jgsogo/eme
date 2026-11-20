//! External catalogue and availability providers.
mod tmdb;
mod trakt_oauth;
mod watchmode;

pub use tmdb::{TmdbClient, TmdbError, TmdbPage};
pub use trakt_oauth::{
    TraktAuthorization, TraktError, TraktOAuthConfig, TraktOAuthService, TraktTokenManager,
};
pub use watchmode::{
    WatchmodeClient, WatchmodeError, WatchmodeSource, filter_subscription_offers,
};
