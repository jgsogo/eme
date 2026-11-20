//! External catalogue and availability providers.
mod tmdb;
mod watchmode;

pub use tmdb::{TmdbClient, TmdbError, TmdbPage};
pub use watchmode::{
    WatchmodeClient, WatchmodeError, WatchmodeSource, filter_subscription_offers,
};
