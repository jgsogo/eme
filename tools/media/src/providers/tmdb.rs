//! TMDB REST API adapter.
//!
//! TMDB response DTOs are private to this module. All public results are mapped
//! to EME's provider-independent media domain types.
use std::{collections::HashMap, sync::Arc};

use chrono::NaiveDate;
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::{ExternalId, MediaItem, MediaItemId, MediaKind};

const DEFAULT_BASE_URL: &str = "https://api.themoviedb.org/3";

/// TMDB catalogue page normalized into EME media items.
#[derive(Debug, Clone, PartialEq)]
pub struct TmdbPage {
    pub items: Vec<MediaItem>,
    pub page: u32,
    pub total_pages: u32,
    pub total_results: u32,
}

/// Errors returned by the TMDB adapter.
#[derive(Debug, thiserror::Error)]
pub enum TmdbError {
    #[error("TMDB API key must not be empty")]
    EmptyApiKey,
    #[error("TMDB request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("TMDB returned HTTP {status}: {message}")]
    Api { status: StatusCode, message: String },
    #[error("TMDB returned an invalid response: {0}")]
    InvalidResponse(String),
}

/// Client for TMDB movie and television catalogue endpoints.
#[derive(Clone)]
pub struct TmdbClient {
    http: Client,
    api_key: String,
    language: String,
    region: String,
    base_url: String,
    genre_cache: Arc<RwLock<HashMap<MediaKind, HashMap<i32, String>>>>,
}

impl TmdbClient {
    /// Create a client using the official TMDB v3 API.
    pub fn new(
        api_key: impl Into<String>,
        language: impl Into<String>,
        region: impl Into<String>,
    ) -> Result<Self, TmdbError> {
        Self::with_base_url(api_key, language, region, DEFAULT_BASE_URL)
    }

    /// Create a client with a custom base URL (useful for controlled test servers).
    pub fn with_base_url(
        api_key: impl Into<String>,
        language: impl Into<String>,
        region: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Result<Self, TmdbError> {
        let api_key = api_key.into();
        if api_key.trim().is_empty() {
            return Err(TmdbError::EmptyApiKey);
        }
        Ok(Self {
            http: Client::new(),
            api_key,
            language: language.into(),
            region: region.into(),
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            genre_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Search movies by title.
    pub async fn search_movies(&self, query: &str, page: u32) -> Result<TmdbPage, TmdbError> {
        self.search(MediaKind::Movie, query, page).await
    }

    /// Search TV shows by title.
    pub async fn search_tv(&self, query: &str, page: u32) -> Result<TmdbPage, TmdbError> {
        self.search(MediaKind::TvShow, query, page).await
    }

    /// Fetch normalized movie details.
    pub async fn movie_details(&self, tmdb_id: u64) -> Result<MediaItem, TmdbError> {
        let dto: TmdbDetails = self
            .request_json(
                &format!("/movie/{tmdb_id}"),
                &[("append_to_response", "release_dates".into())],
            )
            .await?;
        self.map_details(MediaKind::Movie, dto)
    }

    /// Fetch normalized TV-show details.
    pub async fn tv_details(&self, tmdb_id: u64) -> Result<MediaItem, TmdbError> {
        let dto: TmdbDetails = self
            .request_json(
                &format!("/tv/{tmdb_id}"),
                &[("append_to_response", "content_ratings".into())],
            )
            .await?;
        self.map_details(MediaKind::TvShow, dto)
    }

    /// Fetch popular movies, used as a default discovery candidate pool.
    pub async fn popular_movies(&self, page: u32) -> Result<TmdbPage, TmdbError> {
        self.popular(MediaKind::Movie, page).await
    }

    /// Fetch popular TV shows, used as a default discovery candidate pool.
    pub async fn popular_tv(&self, page: u32) -> Result<TmdbPage, TmdbError> {
        self.popular(MediaKind::TvShow, page).await
    }

    async fn popular(&self, kind: MediaKind, page: u32) -> Result<TmdbPage, TmdbError> {
        let path = match kind {
            MediaKind::Movie => "/movie/popular",
            MediaKind::TvShow => "/tv/popular",
        };
        let dto: TmdbListPage = self
            .request_json(path, &[("page", page.max(1).to_string())])
            .await?;
        let genres = self.genre_map(kind).await?;
        Ok(TmdbPage {
            items: dto
                .results
                .into_iter()
                .map(|item| item.into_media_item(kind, &genres))
                .collect(),
            page: dto.page,
            total_pages: dto.total_pages,
            total_results: dto.total_results,
        })
    }

    /// Fetch regional streaming offers from TMDB as a fallback when Watchmode
    /// has no usable offers. Provider IDs and URLs are returned by TMDB.
    pub async fn watch_providers(
        &self,
        tmdb_id: u64,
        kind: MediaKind,
        region: &str,
    ) -> Result<Vec<StreamingOffer>, TmdbError> {
        let path = match kind {
            MediaKind::Movie => format!("/movie/{tmdb_id}/watch/providers"),
            MediaKind::TvShow => format!("/tv/{tmdb_id}/watch/providers"),
        };
        let response: TmdbWatchProviderResponse = self.request_json(&path, &[]).await?;
        let region = region.trim().to_ascii_uppercase();
        let Some(country) = response.results.get(&region) else {
            return Ok(Vec::new());
        };
        let media_item_id = MediaItemId::from_external_id("tmdb", &tmdb_id.to_string());
        let mut offers = Vec::new();
        let groups = [
            (OfferType::Subscription, &country.flatrate),
            (OfferType::Rent, &country.rent),
            (OfferType::Buy, &country.buy),
            (OfferType::Free, &country.free),
            (OfferType::AdSupported, &country.ads),
        ];
        for (offer_type, providers) in groups {
            for provider in providers {
                offers.push(StreamingOffer {
                    media_item_id,
                    service_id: provider.provider_id.to_string(),
                    service_name: provider.provider_name.clone(),
                    offer_type,
                    region: region.clone(),
                    url: country.link.clone().filter(|value| !value.trim().is_empty()),
                });
            }
        }
        Ok(offers)
    }

    /// Fetch TMDB's recommendations for a movie or TV show.
    pub async fn recommendations(
        &self,
        kind: MediaKind,
        tmdb_id: u64,
        page: u32,
    ) -> Result<TmdbPage, TmdbError> {
        let path = match kind {
            MediaKind::Movie => format!("/movie/{tmdb_id}/recommendations"),
            MediaKind::TvShow => format!("/tv/{tmdb_id}/recommendations"),
        };
        let dto: TmdbListPage = self
            .request_json(&path, &[("page", page.max(1).to_string())])
            .await?;
        let genres = self.genre_map(kind).await?;
        Ok(TmdbPage {
            items: dto
                .results
                .into_iter()
                .map(|item| item.into_media_item(kind, &genres))
                .collect(),
            page: dto.page,
            total_pages: dto.total_pages,
            total_results: dto.total_results,
        })
    }

    async fn search(&self, kind: MediaKind, query: &str, page: u32) -> Result<TmdbPage, TmdbError> {
        if query.trim().is_empty() {
            return Err(TmdbError::InvalidResponse(
                "search query must not be empty".into(),
            ));
        }
        let path = match kind {
            MediaKind::Movie => "/search/movie",
            MediaKind::TvShow => "/search/tv",
        };
        let dto: TmdbListPage = self
            .request_json(
                path,
                &[
                    ("query", query.trim().to_owned()),
                    ("page", page.max(1).to_string()),
                    ("region", self.region.clone()),
                ],
            )
            .await?;
        let genres = self.genre_map(kind).await?;
        Ok(TmdbPage {
            items: dto
                .results
                .into_iter()
                .map(|item| item.into_media_item(kind, &genres))
                .collect(),
            page: dto.page,
            total_pages: dto.total_pages,
            total_results: dto.total_results,
        })
    }

    async fn genre_map(&self, kind: MediaKind) -> Result<HashMap<i32, String>, TmdbError> {
        if let Some(cached) = self.genre_cache.read().await.get(&kind).cloned() {
            return Ok(cached);
        }
        let path = match kind {
            MediaKind::Movie => "/genre/movie/list",
            MediaKind::TvShow => "/genre/tv/list",
        };
        let response: TmdbGenreList = self.request_json(path, &[]).await?;
        let genres = response
            .genres
            .into_iter()
            .map(|genre| (genre.id, genre.name))
            .collect::<HashMap<_, _>>();
        self.genre_cache.write().await.insert(kind, genres.clone());
        Ok(genres)
    }

    fn map_details(&self, kind: MediaKind, dto: TmdbDetails) -> Result<MediaItem, TmdbError> {
        let release_date = parse_date(dto.release_date.or(dto.first_air_date))?;
        let genres = dto.genres.into_iter().map(|genre| genre.name).collect();
        let runtime_minutes = dto
            .runtime
            .or_else(|| dto.episode_run_time.first().copied());
        let certification = match kind {
            MediaKind::Movie => dto.release_dates.as_ref().and_then(|dates| {
                dates
                    .results
                    .iter()
                    .find(|country| country.iso_3166_1 == self.region)
                    .and_then(|country| {
                        country
                            .release_dates
                            .iter()
                            .find_map(|release| non_empty(Some(release.certification.clone())))
                    })
            }),
            MediaKind::TvShow => dto.content_ratings.as_ref().and_then(|ratings| {
                ratings
                    .results
                    .iter()
                    .find(|rating| rating.iso_3166_1 == self.region)
                    .and_then(|rating| non_empty(Some(rating.rating.clone())))
            }),
        };
        Ok(MediaItem {
            id: MediaItemId::from_external_id("tmdb", &dto.id.to_string()),
            kind,
            title: dto.title.or(dto.name).unwrap_or_default(),
            original_title: dto.original_title.or(dto.original_name),
            overview: non_empty(dto.overview),
            release_date,
            original_language: non_empty(dto.original_language),
            genres,
            runtime_minutes,
            certification,
            popularity: dto.popularity,
            vote_average: dto.vote_average,
            vote_count: dto.vote_count,
            external_ids: vec![ExternalId {
                provider: "tmdb".into(),
                value: dto.id.to_string(),
            }],
        })
    }

    async fn request_json<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        extra: &[(&str, String)],
    ) -> Result<T, TmdbError> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = self.http.get(url).query(&[
            ("api_key", self.api_key.as_str()),
            ("language", self.language.as_str()),
        ]);
        for (key, value) in extra {
            request = request.query(&[(key, value)]);
        }
        let started = std::time::Instant::now();
        let response = request.send().await?;
        let status = response.status();
        tracing::debug!(
            provider = "tmdb",
            status = status.as_u16(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "media provider request completed"
        );
        if !status.is_success() {
            tracing::warn!(
                provider = "tmdb",
                status = status.as_u16(),
                "media provider request failed"
            );
            let message = response
                .json::<TmdbApiError>()
                .await
                .map(|error| error.status_message)
                .unwrap_or_else(|_| "request failed".into());
            return Err(TmdbError::Api { status, message });
        }
        response.json().await.map_err(TmdbError::Request)
    }
}

fn parse_date(value: Option<String>) -> Result<Option<NaiveDate>, TmdbError> {
    value
        .filter(|value| !value.is_empty())
        .map(|value| {
            NaiveDate::parse_from_str(&value, "%Y-%m-%d")
                .map_err(|error| TmdbError::InvalidResponse(error.to_string()))
        })
        .transpose()
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

#[derive(Debug, Deserialize)]
struct TmdbApiError {
    #[serde(default)]
    status_message: String,
}

#[derive(Debug, Deserialize)]
struct TmdbListPage {
    #[serde(default)]
    page: u32,
    #[serde(default)]
    results: Vec<TmdbListItem>,
    #[serde(default)]
    total_pages: u32,
    #[serde(default)]
    total_results: u32,
}

#[derive(Debug, Deserialize)]
struct TmdbListItem {
    id: u64,
    title: Option<String>,
    name: Option<String>,
    original_title: Option<String>,
    original_name: Option<String>,
    overview: Option<String>,
    release_date: Option<String>,
    first_air_date: Option<String>,
    original_language: Option<String>,
    #[serde(default)]
    genre_ids: Vec<i32>,
    popularity: Option<f32>,
    vote_average: Option<f32>,
    vote_count: Option<u32>,
}

impl TmdbListItem {
    fn into_media_item(self, kind: MediaKind, genres: &HashMap<i32, String>) -> MediaItem {
        MediaItem {
            id: MediaItemId::from_external_id("tmdb", &self.id.to_string()),
            kind,
            title: self.title.or(self.name).unwrap_or_default(),
            original_title: self.original_title.or(self.original_name),
            overview: non_empty(self.overview),
            release_date: self
                .release_date
                .or(self.first_air_date)
                .and_then(|value| NaiveDate::parse_from_str(&value, "%Y-%m-%d").ok()),
            original_language: non_empty(self.original_language),
            genres: self
                .genre_ids
                .into_iter()
                .filter_map(|id| genres.get(&id).cloned())
                .collect(),
            runtime_minutes: None,
            certification: None,
            popularity: self.popularity,
            vote_average: self.vote_average,
            vote_count: self.vote_count,
            external_ids: vec![ExternalId {
                provider: "tmdb".into(),
                value: self.id.to_string(),
            }],
        }
    }
}

#[derive(Debug, Deserialize)]
struct TmdbDetails {
    id: u64,
    title: Option<String>,
    name: Option<String>,
    original_title: Option<String>,
    original_name: Option<String>,
    overview: Option<String>,
    release_date: Option<String>,
    first_air_date: Option<String>,
    original_language: Option<String>,
    runtime: Option<u16>,
    #[serde(default)]
    episode_run_time: Vec<u16>,
    #[serde(default)]
    genres: Vec<TmdbGenre>,
    popularity: Option<f32>,
    vote_average: Option<f32>,
    vote_count: Option<u32>,
    release_dates: Option<TmdbReleaseDates>,
    content_ratings: Option<TmdbContentRatings>,
}

#[derive(Debug, Deserialize)]
struct TmdbWatchProviderResponse {
    #[serde(default)]
    results: HashMap<String, TmdbWatchProviderCountry>,
}

#[derive(Debug, Deserialize)]
struct TmdbWatchProviderCountry {
    link: Option<String>,
    #[serde(default)]
    flatrate: Vec<TmdbWatchProvider>,
    #[serde(default)]
    rent: Vec<TmdbWatchProvider>,
    #[serde(default)]
    buy: Vec<TmdbWatchProvider>,
    #[serde(default)]
    free: Vec<TmdbWatchProvider>,
    #[serde(default)]
    ads: Vec<TmdbWatchProvider>,
}

#[derive(Debug, Deserialize)]
struct TmdbWatchProvider {
    provider_id: u64,
    provider_name: String,
}

#[derive(Debug, Deserialize)]
struct TmdbReleaseDates {
    #[serde(default)]
    results: Vec<TmdbReleaseCountry>,
}

#[derive(Debug, Deserialize)]
struct TmdbReleaseCountry {
    iso_3166_1: String,
    #[serde(default)]
    release_dates: Vec<TmdbReleaseDate>,
}

#[derive(Debug, Deserialize)]
struct TmdbReleaseDate {
    #[serde(default)]
    certification: String,
}

#[derive(Debug, Deserialize)]
struct TmdbContentRatings {
    #[serde(default)]
    results: Vec<TmdbContentRating>,
}

#[derive(Debug, Deserialize)]
struct TmdbContentRating {
    iso_3166_1: String,
    #[serde(default)]
    rating: String,
}

#[derive(Debug, Deserialize)]
struct TmdbGenreList {
    #[serde(default)]
    genres: Vec<TmdbGenre>,
}

#[derive(Debug, Deserialize)]
struct TmdbGenre {
    id: i32,
    name: String,
}

#[cfg(test)]
mod tests {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    use super::*;

    #[tokio::test]
    async fn movie_search_normalizes_metadata_and_resolves_genre_ids() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/genre/movie/list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "genres": [{"id": 18, "name": "Drama"}]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/search/movie"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "page": 1,
                "total_pages": 1,
                "total_results": 1,
                "results": [{
                    "id": 42,
                    "title": "Example",
                    "original_title": "Original Example",
                    "overview": "A short description",
                    "release_date": "2024-01-02",
                    "original_language": "en",
                    "genre_ids": [18]
                }]
            })))
            .mount(&server)
            .await;

        let client = TmdbClient::with_base_url("test-key", "es-ES", "ES", server.uri()).unwrap();
        let page = client.search_movies("Example", 1).await.unwrap();

        assert_eq!(page.total_results, 1);
        assert_eq!(page.items[0].title, "Example");
        assert_eq!(page.items[0].genres, vec!["Drama"]);
        assert_eq!(
            page.items[0].release_date.unwrap().to_string(),
            "2024-01-02"
        );
        assert_eq!(page.items[0].external_ids[0].value, "42");
    }

    #[tokio::test]
    async fn tv_details_normalize_episode_runtime() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/tv/7"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 7,
                "name": "Example Series",
                "original_name": "Original Series",
                "first_air_date": "2020-05-06",
                "original_language": "es",
                "episode_run_time": [44, 46],
                "genres": [{"id": 18, "name": "Drama"}],
                "content_ratings": {"results": [{"iso_3166_1": "ES", "rating": "12"}]}
            })))
            .mount(&server)
            .await;

        let client = TmdbClient::with_base_url("test-key", "es-ES", "ES", server.uri()).unwrap();
        let item = client.tv_details(7).await.unwrap();
        assert_eq!(item.title, "Example Series");
        assert_eq!(item.runtime_minutes, Some(44));
        assert_eq!(item.genres, vec!["Drama"]);
        assert_eq!(item.certification.as_deref(), Some("12"));
    }

    #[tokio::test]
    async fn popular_movies_include_quality_signals_and_normalized_genres() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/genre/movie/list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "genres": [{"id": 35, "name": "Comedy"}]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/movie/popular"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "page": 1,
                "total_pages": 1,
                "total_results": 1,
                "results": [{
                    "id": 42,
                    "title": "Popular",
                    "genre_ids": [35],
                    "popularity": 80.0,
                    "vote_average": 8.1,
                    "vote_count": 900
                }]
            })))
            .mount(&server)
            .await;

        let client = TmdbClient::with_base_url("test-key", "es-ES", "ES", server.uri()).unwrap();
        let page = client.popular_movies(1).await.unwrap();
        assert_eq!(page.items[0].genres, vec!["Comedy"]);
        assert_eq!(page.items[0].vote_average, Some(8.1));
        assert_eq!(page.items[0].vote_count, Some(900));
    }

    #[tokio::test]
    async fn movie_details_extract_spanish_certification() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/movie/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 42,
                "title": "Movie",
                "release_date": "2024-01-02",
                "genres": [{"id": 35, "name": "Comedy"}],
                "release_dates": {
                    "results": [{
                        "iso_3166_1": "ES",
                        "release_dates": [{"certification": "16"}]
                    }]
                }
            })))
            .mount(&server)
            .await;

        let client = TmdbClient::with_base_url("test-key", "es-ES", "ES", server.uri()).unwrap();
        let item = client.movie_details(42).await.unwrap();
        assert_eq!(item.certification.as_deref(), Some("16"));
    }

    #[tokio::test]
    async fn rejects_empty_search_query() {
        let client =
            TmdbClient::with_base_url("test-key", "es-ES", "ES", "http://localhost").unwrap();
        assert!(matches!(
            client.search_movies("  ", 1).await,
            Err(TmdbError::InvalidResponse(_))
        ));
    }
}
