//! Incremental import of one Trakt account's own history, ratings and watchlist.
use agent_storage::UserId;
use chrono::{DateTime, Utc};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    ExternalId, MediaItem, MediaItemId, MediaKind, MediaRating, MediaStore, MediaStoreError,
    MediaWatchEvent, MediaWatchlistItem, ProviderSyncState, TraktError, TraktTokenManager,
    WatchHistorySource,
};

const DEFAULT_API_BASE_URL: &str = "https://api.trakt.tv";

/// Summary of data imported by one profile sync.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TraktSyncSummary {
    pub watched_movies: usize,
    pub watched_shows: usize,
    pub ratings: usize,
    pub watchlist_items: usize,
    pub categories_refreshed: usize,
}

/// Incremental, profile-scoped Trakt data synchronization.
#[derive(Clone)]
pub struct TraktSyncService {
    media: MediaStore,
    token_manager: TraktTokenManager,
    client_id: String,
    api_base_url: String,
    http: Client,
}

impl TraktSyncService {
    pub fn new(
        media: MediaStore,
        token_manager: TraktTokenManager,
        client_id: impl Into<String>,
    ) -> Result<Self, TraktSyncError> {
        Self::with_base_url(media, token_manager, client_id, DEFAULT_API_BASE_URL)
    }

    /// Custom API endpoint supports deterministic tests and compatible deployments.
    pub fn with_base_url(
        media: MediaStore,
        token_manager: TraktTokenManager,
        client_id: impl Into<String>,
        api_base_url: impl Into<String>,
    ) -> Result<Self, TraktSyncError> {
        let client_id = client_id.into();
        if client_id.trim().is_empty() {
            return Err(TraktSyncError::InvalidConfiguration(
                "Trakt client ID must not be empty".into(),
            ));
        }
        let http = Client::builder()
            .user_agent(concat!("EME/", env!("CARGO_PKG_VERSION")))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            media,
            token_manager,
            client_id,
            api_base_url: api_base_url.into().trim_end_matches('/').to_owned(),
            http,
        })
    }

    /// Sync only when Trakt's activity timestamps show a category has changed.
    ///
    /// The caller must supply the authenticated EME owner. The service verifies
    /// that the profile's stored Trakt link belongs to that owner.
    pub async fn sync_profile(
        &self,
        owner_id: UserId,
        profile_id: crate::MediaProfileId,
    ) -> Result<TraktSyncSummary, TraktSyncError> {
        let started = std::time::Instant::now();
        tracing::debug!("starting Trakt media synchronization");
        let link = self
            .media
            .get_external_connection_link(profile_id, "trakt")
            .await?
            .ok_or(TraktSyncError::NotLinked)?;
        if link.owner_id != owner_id {
            return Err(TraktSyncError::Forbidden);
        }
        let access_token = self
            .token_manager
            .access_token(owner_id, link.external_connection_id)
            .await?;

        let activities: TraktLastActivities = self
            .get_json("/sync/last_activities", &access_token)
            .await?;
        let cursor = TraktSyncCursor::from(&activities);
        let previous = self
            .media
            .get_provider_sync_state(profile_id, "trakt")
            .await?
            .and_then(|state| state.cursor)
            .map(|value| {
                serde_json::from_str::<TraktSyncCursor>(&value)
                    .map_err(|error| TraktSyncError::InvalidResponse(error.to_string()))
            })
            .transpose()?;

        let first_sync = previous.is_none();
        let previous = previous.unwrap_or_default();
        let movies_watched_changed =
            first_sync || previous.movies_watched_at != cursor.movies_watched_at;
        let shows_watched_changed = first_sync
            || previous.shows_watched_at != cursor.shows_watched_at
            || previous.episodes_watched_at != cursor.episodes_watched_at;
        let ratings_changed = first_sync
            || previous.movies_rated_at != cursor.movies_rated_at
            || previous.shows_rated_at != cursor.shows_rated_at;
        let watchlist_changed = first_sync
            || previous.movies_watchlisted_at != cursor.movies_watchlisted_at
            || previous.shows_watchlisted_at != cursor.shows_watchlisted_at;

        let mut summary = TraktSyncSummary::default();
        if movies_watched_changed {
            let watched: Vec<TraktWatchedMovie> =
                self.get_json("/sync/watched/movies", &access_token).await?;
            for row in watched {
                let item = row.movie.into_media_item(MediaKind::Movie)?;
                self.media.save_media_item(&item).await?;
                let watched_at = row
                    .last_watched_at
                    .ok_or(TraktSyncError::MissingActivityTimestamp)?;
                self.media
                    .record_watch_event(&MediaWatchEvent {
                        profile_id,
                        media_item_id: item.id,
                        watched_at,
                        source: WatchHistorySource::Trakt,
                    })
                    .await?;
                summary.watched_movies += 1;
            }
            summary.categories_refreshed += 1;
        }

        if shows_watched_changed {
            let watched: Vec<TraktWatchedShow> =
                self.get_json("/sync/watched/shows", &access_token).await?;
            for row in watched {
                let item = row.show.into_media_item(MediaKind::TvShow)?;
                self.media.save_media_item(&item).await?;
                let watched_at = row
                    .last_watched_at
                    .or_else(|| latest_episode_watch(&row.seasons))
                    .ok_or(TraktSyncError::MissingActivityTimestamp)?;
                self.media
                    .record_watch_event(&MediaWatchEvent {
                        profile_id,
                        media_item_id: item.id,
                        watched_at,
                        source: WatchHistorySource::Trakt,
                    })
                    .await?;
                summary.watched_shows += 1;
            }
            summary.categories_refreshed += 1;
        }

        if ratings_changed {
            let movies: Vec<TraktRatedMovie> =
                self.get_json("/sync/ratings/movies", &access_token).await?;
            let shows: Vec<TraktRatedShow> =
                self.get_json("/sync/ratings/shows", &access_token).await?;
            let mut ratings = Vec::with_capacity(movies.len() + shows.len());
            for row in movies {
                let item = row.movie.into_media_item(MediaKind::Movie)?;
                self.media.save_media_item(&item).await?;
                ratings.push(normalize_rating(
                    profile_id,
                    item.id,
                    row.rating,
                    row.rated_at,
                )?);
            }
            for row in shows {
                let item = row.show.into_media_item(MediaKind::TvShow)?;
                self.media.save_media_item(&item).await?;
                ratings.push(normalize_rating(
                    profile_id,
                    item.id,
                    row.rating,
                    row.rated_at,
                )?);
            }
            summary.ratings = ratings.len();
            self.media
                .replace_ratings(profile_id, WatchHistorySource::Trakt, &ratings)
                .await?;
            summary.categories_refreshed += 1;
        }

        if watchlist_changed {
            let movies: Vec<TraktWatchlistMovie> = self
                .get_json("/sync/watchlist/movies", &access_token)
                .await?;
            let shows: Vec<TraktWatchlistShow> = self
                .get_json("/sync/watchlist/shows", &access_token)
                .await?;
            let mut items = Vec::with_capacity(movies.len() + shows.len());
            for row in movies {
                let item = row.movie.into_media_item(MediaKind::Movie)?;
                self.media.save_media_item(&item).await?;
                items.push(MediaWatchlistItem {
                    profile_id,
                    media_item_id: item.id,
                    added_at: row.listed_at,
                    source: WatchHistorySource::Trakt,
                });
            }
            for row in shows {
                let item = row.show.into_media_item(MediaKind::TvShow)?;
                self.media.save_media_item(&item).await?;
                items.push(MediaWatchlistItem {
                    profile_id,
                    media_item_id: item.id,
                    added_at: row.listed_at,
                    source: WatchHistorySource::Trakt,
                });
            }
            summary.watchlist_items = items.len();
            self.media
                .replace_watchlist(profile_id, WatchHistorySource::Trakt, &items)
                .await?;
            summary.categories_refreshed += 1;
        }

        let state = ProviderSyncState {
            profile_id,
            provider: "trakt".into(),
            cursor: Some(
                serde_json::to_string(&cursor)
                    .map_err(|error| TraktSyncError::InvalidResponse(error.to_string()))?,
            ),
            last_synced_at: Some(Utc::now()),
            status: "complete".into(),
        };
        self.media.save_provider_sync_state(&state).await?;
        tracing::info!(
            watched_movies = summary.watched_movies,
            watched_shows = summary.watched_shows,
            ratings = summary.ratings,
            watchlist_items = summary.watchlist_items,
            categories_refreshed = summary.categories_refreshed,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "Trakt media synchronization completed"
        );
        Ok(summary)
    }

    /// Add or remove a title from the linked Trakt account's watchlist.
    ///
    /// The provider is mutated first. The local Trakt mirror is changed only
    /// after Trakt confirms the request, so a failed write cannot masquerade
    /// as a successful remote update. Repeating either operation is safe.
    pub async fn set_watchlist_item(
        &self,
        owner_id: UserId,
        profile_id: crate::MediaProfileId,
        item: &MediaItem,
        add: bool,
    ) -> Result<(), TraktSyncError> {
        let access_token = self.access_token_for_profile(owner_id, profile_id).await?;
        let tmdb_id = item
            .external_ids
            .iter()
            .find(|external_id| external_id.provider == "tmdb")
            .and_then(|external_id| external_id.value.parse::<u64>().ok())
            .ok_or(TraktSyncError::MissingExternalId)?;
        let collection = match item.kind {
            MediaKind::Movie => "movies",
            MediaKind::TvShow => "shows",
        };
        let payload = match item.kind {
            MediaKind::Movie => json!({"movies":[{"ids":{"tmdb":tmdb_id}}]}),
            MediaKind::TvShow => json!({"shows":[{"ids":{"tmdb":tmdb_id}}]}),
        };
        let path = if add {
            "/sync/watchlist"
        } else {
            "/sync/watchlist/remove"
        };
        let response = self.post_json(path, &payload, &access_token).await?;
        ensure_trakt_item_found(&response, collection)?;

        self.media.save_media_item(item).await?;
        if add {
            let existing = self.media.list_watchlist(profile_id).await?;
            if !existing.iter().any(|entry| {
                entry.media_item_id == item.id && entry.source == WatchHistorySource::Trakt
            }) {
                self.media
                    .save_watchlist_item(&MediaWatchlistItem {
                        profile_id,
                        media_item_id: item.id,
                        added_at: Utc::now(),
                        source: WatchHistorySource::Trakt,
                    })
                    .await?;
            }
        } else {
            let mut existing = self.media.list_watchlist(profile_id).await?;
            existing.retain(|entry| {
                entry.media_item_id != item.id && entry.source == WatchHistorySource::Trakt
            });
            self.media
                .replace_watchlist(profile_id, WatchHistorySource::Trakt, &existing)
                .await?;
        }
        Ok(())
    }

    /// Write an EME rating through to Trakt, then persist its EME provenance.
    ///
    /// EME's 0..=5 scale maps linearly to Trakt's integer 1..=10 scale:
    /// 0 maps to 1 and 5 maps to 10. Trakt's coarser scale is necessarily
    /// quantized; inbound ratings use the inverse mapping.
    pub async fn push_rating(
        &self,
        owner_id: UserId,
        profile_id: crate::MediaProfileId,
        item: &MediaItem,
        rating: &MediaRating,
    ) -> Result<(), TraktSyncError> {
        if rating.profile_id != profile_id
            || rating.media_item_id != item.id
            || rating.source != WatchHistorySource::Eme
            || !rating.value.is_finite()
            || !(0.0..=5.0).contains(&rating.value)
        {
            return Err(TraktSyncError::InvalidResponse(
                "only a valid EME rating for the target profile can be synchronized".into(),
            ));
        }
        let access_token = self.access_token_for_profile(owner_id, profile_id).await?;
        let tmdb_id = item
            .external_ids
            .iter()
            .find(|external_id| external_id.provider == "tmdb")
            .and_then(|external_id| external_id.value.parse::<u64>().ok())
            .ok_or(TraktSyncError::MissingExternalId)?;
        let trakt_rating = trakt_rating_from_eme(rating.value)?;
        let rated_at = rating.rated_at.to_rfc3339();
        let collection = match item.kind {
            MediaKind::Movie => "movies",
            MediaKind::TvShow => "shows",
        };
        let payload = match item.kind {
            MediaKind::Movie => json!({"movies":[{
                "ids":{"tmdb":tmdb_id},"rating":trakt_rating,"rated_at":rated_at
            }]}),
            MediaKind::TvShow => json!({"shows":[{
                "ids":{"tmdb":tmdb_id},"rating":trakt_rating,"rated_at":rated_at
            }]}),
        };
        let response = self
            .post_json("/sync/ratings", &payload, &access_token)
            .await?;
        ensure_trakt_item_found(&response, collection)?;
        self.media.save_media_item(item).await?;
        self.media.save_rating(rating).await?;
        Ok(())
    }

    async fn access_token_for_profile(
        &self,
        owner_id: UserId,
        profile_id: crate::MediaProfileId,
    ) -> Result<String, TraktSyncError> {
        let link = self
            .media
            .get_external_connection_link(profile_id, "trakt")
            .await?
            .ok_or(TraktSyncError::NotLinked)?;
        if link.owner_id != owner_id {
            return Err(TraktSyncError::Forbidden);
        }
        Ok(self
            .token_manager
            .access_token(owner_id, link.external_connection_id)
            .await?)
    }

    async fn post_json(
        &self,
        path: &str,
        payload: &Value,
        access_token: &str,
    ) -> Result<Value, TraktSyncError> {
        let started = std::time::Instant::now();
        let response = self
            .http
            .post(format!("{}{}", self.api_base_url, path))
            .bearer_auth(access_token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header("trakt-api-version", "2")
            .header("trakt-api-key", &self.client_id)
            .json(payload)
            .send()
            .await?;
        let status = response.status();
        tracing::debug!(
            provider = "trakt",
            status = status.as_u16(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "media provider write completed"
        );
        if status == StatusCode::TOO_MANY_REQUESTS {
            let retry_after_seconds = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse().ok());
            return Err(TraktSyncError::RateLimited {
                retry_after_seconds,
            });
        }
        if !status.is_success() {
            let message = response
                .text()
                .await
                .unwrap_or_else(|_| "request failed".into());
            return Err(TraktSyncError::Api { status, message });
        }
        response.json().await.map_err(TraktSyncError::Request)
    }

    async fn get_json<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        access_token: &str,
    ) -> Result<T, TraktSyncError> {
        let started = std::time::Instant::now();
        let response = self
            .http
            .get(format!("{}{}", self.api_base_url, path))
            .bearer_auth(access_token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header("trakt-api-version", "2")
            .header("trakt-api-key", &self.client_id)
            .send()
            .await?;
        let status = response.status();
        tracing::debug!(
            provider = "trakt",
            status = status.as_u16(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "media provider request completed"
        );
        if status == StatusCode::TOO_MANY_REQUESTS {
            tracing::warn!(provider = "trakt", "media provider rate limit reached");
            let retry_after_seconds = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse().ok());
            return Err(TraktSyncError::RateLimited {
                retry_after_seconds,
            });
        }
        if !status.is_success() {
            tracing::warn!(
                provider = "trakt",
                status = status.as_u16(),
                "media provider request failed"
            );
            let message = response
                .text()
                .await
                .unwrap_or_else(|_| "request failed".into());
            return Err(TraktSyncError::Api { status, message });
        }
        response.json().await.map_err(TraktSyncError::Request)
    }
}

fn ensure_trakt_item_found(
    response: &Value,
    collection: &str,
) -> Result<(), TraktSyncError> {
    let missing = response
        .get("not_found")
        .and_then(|value| value.get(collection))
        .and_then(Value::as_array)
        .is_some_and(|items| !items.is_empty());
    if missing {
        return Err(TraktSyncError::InvalidResponse(format!(
            "Trakt could not resolve the requested {collection} item"
        )));
    }
    Ok(())
}

fn trakt_rating_from_eme(value: f32) -> Result<u8, TraktSyncError> {
    if !value.is_finite() || !(0.0..=5.0).contains(&value) {
        return Err(TraktSyncError::InvalidResponse(
            "EME rating must be between 0 and 5".into(),
        ));
    }
    Ok((1.0 + value * 9.0 / 5.0).round().clamp(1.0, 10.0) as u8)
}

fn normalize_rating(
    profile_id: crate::MediaProfileId,
    media_item_id: MediaItemId,
    rating: u8,
    rated_at: DateTime<Utc>,
) -> Result<MediaRating, TraktSyncError> {
    if !(1..=10).contains(&rating) {
        return Err(TraktSyncError::InvalidResponse(format!(
            "Trakt rating must be between 1 and 10, got {rating}"
        )));
    }
    Ok(MediaRating {
        profile_id,
        media_item_id,
        value: (f32::from(rating) - 1.0) * (5.0 / 9.0),
        rated_at,
        source: WatchHistorySource::Trakt,
    })
}

fn latest_episode_watch(seasons: &[TraktWatchedSeason]) -> Option<DateTime<Utc>> {
    seasons
        .iter()
        .flat_map(|season| season.episodes.iter())
        .filter_map(|episode| episode.last_watched_at)
        .max()
}

#[derive(Debug, thiserror::Error)]
pub enum TraktSyncError {
    #[error("Trakt profile is not linked")]
    NotLinked,
    #[error("Trakt connection does not belong to the authenticated EME user")]
    Forbidden,
    #[error("invalid Trakt sync configuration: {0}")]
    InvalidConfiguration(String),
    #[error("Trakt returned HTTP {status}: {message}")]
    Api { status: StatusCode, message: String },
    #[error("Trakt rate limit reached (retry after {retry_after_seconds:?} seconds)")]
    RateLimited { retry_after_seconds: Option<u64> },
    #[error("Trakt sync response is invalid: {0}")]
    InvalidResponse(String),
    #[error("Trakt item did not include a supported external identifier")]
    MissingExternalId,
    #[error("Trakt activity did not include a usable timestamp")]
    MissingActivityTimestamp,
    #[error("Trakt request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Trakt token refresh failed: {0}")]
    Authentication(#[from] TraktError),
    #[error("media persistence failed: {0}")]
    Storage(#[from] MediaStoreError),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct TraktSyncCursor {
    movies_watched_at: Option<String>,
    shows_watched_at: Option<String>,
    episodes_watched_at: Option<String>,
    movies_rated_at: Option<String>,
    shows_rated_at: Option<String>,
    movies_watchlisted_at: Option<String>,
    shows_watchlisted_at: Option<String>,
}

impl From<&TraktLastActivities> for TraktSyncCursor {
    fn from(value: &TraktLastActivities) -> Self {
        Self {
            movies_watched_at: value.movies.watched_at.clone(),
            shows_watched_at: value.shows.watched_at.clone(),
            episodes_watched_at: value.episodes.watched_at.clone(),
            movies_rated_at: value.movies.rated_at.clone(),
            shows_rated_at: value.shows.rated_at.clone(),
            movies_watchlisted_at: value.movies.watchlisted_at.clone(),
            shows_watchlisted_at: value.shows.watchlisted_at.clone(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct TraktLastActivities {
    #[serde(default)]
    movies: TraktActivityBucket,
    #[serde(default)]
    shows: TraktActivityBucket,
    #[serde(default)]
    episodes: TraktActivityBucket,
}

#[derive(Debug, Default, Deserialize)]
struct TraktActivityBucket {
    watched_at: Option<String>,
    rated_at: Option<String>,
    watchlisted_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TraktWatchedMovie {
    last_watched_at: Option<DateTime<Utc>>,
    movie: TraktMedia,
}

#[derive(Debug, Deserialize)]
struct TraktWatchedShow {
    last_watched_at: Option<DateTime<Utc>>,
    #[serde(default)]
    seasons: Vec<TraktWatchedSeason>,
    show: TraktMedia,
}

#[derive(Debug, Deserialize)]
struct TraktWatchedSeason {
    #[serde(default)]
    episodes: Vec<TraktWatchedEpisode>,
}

#[derive(Debug, Deserialize)]
struct TraktWatchedEpisode {
    last_watched_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
struct TraktRatedMovie {
    rating: u8,
    rated_at: DateTime<Utc>,
    movie: TraktMedia,
}

#[derive(Debug, Deserialize)]
struct TraktRatedShow {
    rating: u8,
    rated_at: DateTime<Utc>,
    show: TraktMedia,
}

#[derive(Debug, Deserialize)]
struct TraktWatchlistMovie {
    listed_at: DateTime<Utc>,
    movie: TraktMedia,
}

#[derive(Debug, Deserialize)]
struct TraktWatchlistShow {
    listed_at: DateTime<Utc>,
    show: TraktMedia,
}

#[derive(Debug, Deserialize)]
struct TraktMedia {
    title: String,
    #[serde(default)]
    ids: TraktIds,
}

#[derive(Debug, Default, Deserialize)]
struct TraktIds {
    trakt: Option<u64>,
    tmdb: Option<u64>,
    imdb: Option<String>,
}

impl TraktMedia {
    fn into_media_item(self, kind: MediaKind) -> Result<MediaItem, TraktSyncError> {
        let mut external_ids = Vec::new();
        if let Some(id) = self.ids.tmdb {
            external_ids.push(ExternalId {
                provider: "tmdb".into(),
                value: id.to_string(),
            });
        }
        if let Some(id) = self.ids.trakt {
            external_ids.push(ExternalId {
                provider: "trakt".into(),
                value: id.to_string(),
            });
        }
        if let Some(id) = self.ids.imdb {
            external_ids.push(ExternalId {
                provider: "imdb".into(),
                value: id,
            });
        }
        let primary = external_ids
            .iter()
            .find(|id| id.provider == "tmdb")
            .or_else(|| external_ids.iter().find(|id| id.provider == "trakt"))
            .or_else(|| external_ids.iter().find(|id| id.provider == "imdb"))
            .ok_or(TraktSyncError::MissingExternalId)?;
        Ok(MediaItem {
            id: MediaItemId::from_external_id(&primary.provider, &primary.value),
            kind,
            title: self.title,
            original_title: None,
            overview: None,
            release_date: None,
            original_language: None,
            genres: Vec::new(),
            runtime_minutes: None,
            certification: None,
            popularity: None,
            vote_average: None,
            vote_count: None,
            external_ids,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::Arc};

    use agent_storage::{
        ExternalConnectionStore, NewExternalConnection, SqliteExternalConnectionStore,
    };
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path},
    };

    use super::*;

    #[test]
    fn eme_and_trakt_rating_scales_map_endpoints_and_midpoints() {
        assert_eq!(trakt_rating_from_eme(0.0).unwrap(), 1);
        assert_eq!(trakt_rating_from_eme(2.5).unwrap(), 6);
        assert_eq!(trakt_rating_from_eme(5.0).unwrap(), 10);
        assert!(trakt_rating_from_eme(5.1).is_err());

        let profile_id = crate::MediaProfileId::new();
        let media_item_id = MediaItemId::from_external_id("tmdb", "42");
        let zero = normalize_rating(profile_id, media_item_id, 1, Utc::now()).unwrap();
        let five = normalize_rating(profile_id, media_item_id, 10, Utc::now()).unwrap();
        assert_eq!(zero.value, 0.0);
        assert_eq!(five.value, 5.0);
    }

    #[tokio::test]
    async fn sync_imports_and_then_skips_unchanged_trakt_categories() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "access-token",
                "refresh_token": "rotated-refresh",
                "expires_in": 604800,
                "scope": "private"
            })))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/sync/last_activities"))
            .and(header("Authorization", "Bearer access-token"))
            .and(header(
                "User-Agent",
                concat!("EME/", env!("CARGO_PKG_VERSION")),
            ))
            .and(header("Content-Type", "application/json"))
            .and(header("trakt-api-key", "client"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "movies": {
                    "watched_at": "2025-01-01T00:00:00Z",
                    "rated_at": "2025-01-02T00:00:00Z",
                    "watchlisted_at": "2025-01-03T00:00:00Z"
                },
                "shows": {
                    "rated_at": "2025-01-04T00:00:00Z",
                    "watchlisted_at": "2025-01-05T00:00:00Z"
                },
                "episodes": {
                    "watched_at": "2025-01-06T00:00:00Z"
                }
            })))
            .expect(2)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/sync/watched/movies"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                "last_watched_at": "2025-01-01T00:00:00Z",
                "movie": {"title": "Movie", "ids": {"trakt": 1, "tmdb": 100, "imdb": "tt0000100"}}
            }])))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/sync/watched/shows"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                    "last_watched_at": "2025-01-06T00:00:00Z",
                    "show": {"title": "Show", "ids": {"trakt": 2, "tmdb": 200}}
                }])),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/sync/ratings/movies"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                    "rating": 8,
                    "rated_at": "2025-01-02T00:00:00Z",
                    "movie": {"title": "Movie", "ids": {"trakt": 1, "tmdb": 100}}
                }])),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/sync/ratings/shows"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                    "rating": 10,
                    "rated_at": "2025-01-04T00:00:00Z",
                    "show": {"title": "Show", "ids": {"trakt": 2, "tmdb": 200}}
                }])),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/sync/watchlist/movies"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                    "listed_at": "2025-01-03T00:00:00Z",
                    "movie": {"title": "Movie", "ids": {"trakt": 1, "tmdb": 100}}
                }])),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/sync/watchlist/shows"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                    "listed_at": "2025-01-05T00:00:00Z",
                    "show": {"title": "Show", "ids": {"trakt": 2, "tmdb": 200}}
                }])),
            )
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/sync/watchlist"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "added": {"movies": 1}
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/sync/watchlist/remove"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "deleted": {"movies": 1}
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/sync/ratings"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "added": {"movies": 1}
            })))
            .expect(1)
            .mount(&server)
            .await;

        let media = MediaStore::open_in_memory().unwrap();
        let family_id = agent_storage::FamilyId::new();
        let mut profile = crate::MediaProfile::new(family_id, "Viewer");
        let owner_id = UserId::new();
        profile.user_id = Some(owner_id);
        media.save_profile(&profile).await.unwrap();

        let db_path =
            std::env::temp_dir().join(format!("media-trakt-sync-{}.db", uuid::Uuid::new_v4()));
        let external = Arc::new(SqliteExternalConnectionStore::open(&db_path, [10; 32]).unwrap());
        let connection = external
            .create_connection(NewExternalConnection {
                owner_id,
                provider: "trakt".into(),
                provider_subject: "viewer".into(),
                display_email: None,
                refresh_token: "initial-refresh".into(),
                granted_scopes: vec!["private".into()],
            })
            .await
            .unwrap();
        media
            .save_external_connection_link(&crate::MediaExternalConnectionLink {
                owner_id,
                family_id: Some(family_id),
                profile_id: profile.id,
                provider: "trakt".into(),
                external_connection_id: connection.id,
                provider_subject: "viewer".into(),
                display_name: None,
                linked_at: Utc::now(),
            })
            .await
            .unwrap();

        let external_trait: Arc<dyn ExternalConnectionStore> = external.clone();
        let oauth_config = crate::TraktOAuthConfig::with_endpoints(
            "client",
            "http://localhost/callback",
            format!("{}/authorize", server.uri()),
            format!("{}/oauth/token", server.uri()),
            server.uri(),
        )
        .unwrap();
        let token_manager = TraktTokenManager::new(oauth_config, external_trait).unwrap();
        let sync =
            TraktSyncService::with_base_url(media.clone(), token_manager, "client", server.uri())
                .unwrap();

        let first = sync.sync_profile(owner_id, profile.id).await.unwrap();
        assert_eq!(first.watched_movies, 1);
        assert_eq!(first.watched_shows, 1);
        assert_eq!(first.ratings, 2);
        assert_eq!(first.watchlist_items, 2);
        assert_eq!(media.list_watch_history(profile.id).await.unwrap().len(), 2);
        assert_eq!(media.list_ratings(profile.id).await.unwrap().len(), 2);
        assert_eq!(media.list_watchlist(profile.id).await.unwrap().len(), 2);

        let movie = media
            .get_media_item(MediaItemId::from_external_id("tmdb", "100"))
            .await
            .unwrap()
            .unwrap();
        sync.set_watchlist_item(owner_id, profile.id, &movie, false)
            .await
            .unwrap();
        assert_eq!(media.list_watchlist(profile.id).await.unwrap().len(), 1);
        sync.set_watchlist_item(owner_id, profile.id, &movie, true)
            .await
            .unwrap();
        assert_eq!(media.list_watchlist(profile.id).await.unwrap().len(), 2);

        let eme_rating = MediaRating {
            profile_id: profile.id,
            media_item_id: movie.id,
            value: 4.0,
            rated_at: Utc::now(),
            source: WatchHistorySource::Eme,
        };
        sync.push_rating(owner_id, profile.id, &movie, &eme_rating)
            .await
            .unwrap();
        assert!(
            media
                .list_ratings(profile.id)
                .await
                .unwrap()
                .iter()
                .any(|rating| rating.source == WatchHistorySource::Eme)
        );

        let second = sync.sync_profile(owner_id, profile.id).await.unwrap();
        assert_eq!(second, TraktSyncSummary::default());

        drop(sync);
        drop(external);
        let _ = fs::remove_file(db_path);
    }
}
