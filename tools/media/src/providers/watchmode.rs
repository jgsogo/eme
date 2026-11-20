//! Watchmode streaming availability adapter.
//!
//! Authentication uses the X-API-Key header so credentials do not appear in
//! request URLs or HTTP query logs. Provider source IDs are read from API
//! responses; no country-specific source IDs are hard-coded.
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use reqwest::{Client, StatusCode};
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::{MediaKind, OfferType, StreamingOffer, StreamingService};

const DEFAULT_BASE_URL: &str = "https://api.watchmode.com/v1";

/// A Watchmode source/service discovered from the provider's reference API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchmodeSource {
    pub id: u64,
    pub name: String,
    pub source_type: Option<String>,
}

/// Errors returned by the Watchmode adapter.
#[derive(Debug, thiserror::Error)]
pub enum WatchmodeError {
    #[error("Watchmode API key must not be empty")]
    EmptyApiKey,
    #[error("Watchmode request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Watchmode returned HTTP {status}: {message}")]
    Api { status: StatusCode, message: String },
    #[error("Watchmode rate limit reached (retry after {retry_after_seconds:?} seconds)")]
    RateLimited { retry_after_seconds: Option<u64> },
    #[error("Watchmode returned an unsupported offer type: {0}")]
    UnsupportedOfferType(String),
    #[error("Watchmode returned an invalid response: {0}")]
    InvalidResponse(String),
}

#[derive(Clone)]
pub struct WatchmodeClient {
    http: Client,
    api_key: String,
    base_url: String,
    cache_ttl: Duration,
    offers_cache: Arc<RwLock<HashMap<String, (Instant, Vec<StreamingOffer>)>>>,
}

impl WatchmodeClient {
    /// Create a client with a conservative in-memory availability cache.
    pub fn new(api_key: impl Into<String>) -> Result<Self, WatchmodeError> {
        Self::with_options(api_key, DEFAULT_BASE_URL, Duration::from_secs(6 * 60 * 60))
    }

    /// Configure the endpoint and in-memory availability cache lifetime.
    pub fn with_options(
        api_key: impl Into<String>,
        base_url: impl Into<String>,
        cache_ttl: Duration,
    ) -> Result<Self, WatchmodeError> {
        let api_key = api_key.into();
        if api_key.trim().is_empty() {
            return Err(WatchmodeError::EmptyApiKey);
        }
        Ok(Self {
            http: Client::new(),
            api_key,
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            cache_ttl,
            offers_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Search Watchmode by a TMDB movie or TV ID and fetch its regional offers.
    ///
    /// The TMDB identifier is used to resolve the Watchmode title ID first.
    /// This avoids assuming that IDs are shared across catalogue providers.
    pub async fn offers_for_tmdb_id(
        &self,
        tmdb_id: u64,
        kind: MediaKind,
        region: &str,
    ) -> Result<Vec<StreamingOffer>, WatchmodeError> {
        let region = normalize_region(region)?;
        let cache_key = format!("{}:{}:{}", kind_key(kind), tmdb_id, region);
        if let Some((created, offers)) = self.offers_cache.read().await.get(&cache_key) {
            if created.elapsed() < self.cache_ttl {
                return Ok(offers.clone());
            }
        }

        let search_field = match kind {
            MediaKind::Movie => "tmdb_movie_id",
            MediaKind::TvShow => "tmdb_tv_id",
        };
        let search: WatchmodeSearchResponse = self
            .request_json(
                "/search/",
                &[
                    ("search_field", search_field.to_owned()),
                    ("search_value", tmdb_id.to_string()),
                ],
            )
            .await?;
        let Some(title) = search.title_results.into_iter().next() else {
            return Ok(Vec::new());
        };

        let media_item_id = crate::MediaItemId::from_external_id("tmdb", &tmdb_id.to_string());
        let offers = self
            .sources_for_title(title.id, media_item_id, &region)
            .await?;
        self.offers_cache
            .write()
            .await
            .insert(cache_key, (Instant::now(), offers.clone()));
        Ok(offers)
    }

    /// Fetch offers for a Watchmode title ID in one region.
    pub async fn sources_for_title(
        &self,
        watchmode_id: u64,
        media_item_id: crate::MediaItemId,
        region: &str,
    ) -> Result<Vec<StreamingOffer>, WatchmodeError> {
        let region = normalize_region(region)?;
        let path = format!("/title/{watchmode_id}/sources/");
        let rows: Vec<WatchmodeOfferDto> = self
            .request_json(&path, &[("regions", region.clone())])
            .await?;
        rows.into_iter()
            .map(|row| {
                Ok(StreamingOffer {
                    media_item_id,
                    service_id: row.source_id.to_string(),
                    service_name: row.name,
                    offer_type: parse_offer_type(&row.source_type)?,
                    region: row.region.unwrap_or_else(|| region.clone()),
                    url: non_empty(row.web_url),
                })
            })
            .collect()
    }

    /// List source metadata for a region. IDs and names come from Watchmode.
    pub async fn list_sources(&self, region: &str) -> Result<Vec<WatchmodeSource>, WatchmodeError> {
        let region = normalize_region(region)?;
        let rows: Vec<WatchmodeSourceDto> = self
            .request_json("/sources/", &[("regions", region)])
            .await?;
        Ok(rows
            .into_iter()
            .map(|source| WatchmodeSource {
                id: source.id,
                name: source.name,
                source_type: source.source_type,
            })
            .collect())
    }

    async fn request_json<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        extra: &[(&str, String)],
    ) -> Result<T, WatchmodeError> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = self.http.get(url).header("X-API-Key", &self.api_key);
        for (key, value) in extra {
            request = request.query(&[(key, value)]);
        }
        let started = Instant::now();
        let response = request.send().await?;
        let status = response.status();
        tracing::debug!(
            provider = "watchmode",
            status = status.as_u16(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "media provider request completed"
        );
        if status == StatusCode::TOO_MANY_REQUESTS {
            let retry_after_seconds = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse().ok());
            return Err(WatchmodeError::RateLimited {
                retry_after_seconds,
            });
        }
        if !status.is_success() {
            let message = response
                .text()
                .await
                .unwrap_or_else(|_| "request failed".into());
            return Err(WatchmodeError::Api { status, message });
        }
        response.json().await.map_err(WatchmodeError::Request)
    }
}

/// Keep only offers matching the household's configured subscriptions.
pub fn filter_subscription_offers(
    offers: Vec<StreamingOffer>,
    subscriptions: &[StreamingService],
) -> Vec<StreamingOffer> {
    offers
        .into_iter()
        .filter(|offer| {
            offer.offer_type == OfferType::Subscription
                && subscriptions
                    .iter()
                    .any(|service| service_matches(*service, &offer.service_name))
        })
        .collect()
}

fn service_matches(service: StreamingService, provider_name: &str) -> bool {
    let normalized = normalize_name(provider_name);
    match service {
        StreamingService::Netflix => normalized == "netflix",
        StreamingService::PrimeVideo => {
            normalized == "prime video" || normalized == "amazon prime video"
        }
        StreamingService::DisneyPlus => normalized == "disney+" || normalized == "disney plus",
        StreamingService::HboMax => normalized == "hbo max" || normalized == "max",
    }
}

fn normalize_name(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_region(region: &str) -> Result<String, WatchmodeError> {
    let region = region.trim().to_ascii_uppercase();
    if region.len() != 2 || !region.bytes().all(|byte| byte.is_ascii_uppercase()) {
        return Err(WatchmodeError::InvalidResponse(
            "region must be a two-letter ISO 3166-1 country code".into(),
        ));
    }
    Ok(region)
}

fn kind_key(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Movie => "movie",
        MediaKind::TvShow => "tv",
    }
}

fn parse_offer_type(value: &str) -> Result<OfferType, WatchmodeError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "sub" | "subscription" => Ok(OfferType::Subscription),
        "rent" => Ok(OfferType::Rent),
        "buy" | "purchase" => Ok(OfferType::Buy),
        "free" => Ok(OfferType::Free),
        "tv" | "tve" | "tv_provider" => Ok(OfferType::TvProvider),
        other => Err(WatchmodeError::UnsupportedOfferType(other.to_owned())),
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

#[derive(Debug, Deserialize)]
struct WatchmodeSearchResponse {
    #[serde(default)]
    title_results: Vec<WatchmodeSearchTitle>,
}

#[derive(Debug, Deserialize)]
struct WatchmodeSearchTitle {
    id: u64,
}

#[derive(Debug, Deserialize)]
struct WatchmodeOfferDto {
    source_id: u64,
    name: String,
    #[serde(rename = "type")]
    source_type: String,
    region: Option<String>,
    web_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WatchmodeSourceDto {
    id: u64,
    name: String,
    #[serde(rename = "type")]
    source_type: Option<String>,
}

#[cfg(test)]
mod tests {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path, query_param},
    };

    use super::*;

    #[tokio::test]
    async fn resolves_tmdb_id_and_maps_regional_subscription_offers() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/search/"))
            .and(header("X-API-Key", "test-key"))
            .and(query_param("search_field", "tmdb_movie_id"))
            .and(query_param("search_value", "42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "title_results": [{"id": 9001, "name": "Example", "type": "movie"}]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/title/9001/sources/"))
            .and(query_param("regions", "ES"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"source_id": 203, "name": "Netflix", "type": "sub", "region": "ES", "web_url": "https://netflix.example/title"},
                {"source_id": 26, "name": "Amazon Prime Video", "type": "sub", "region": "ES", "web_url": "https://prime.example/title"},
                {"source_id": 203, "name": "Netflix", "type": "rent", "region": "ES", "web_url": "https://netflix.example/rent"}
            ])))
            .mount(&server)
            .await;

        let client =
            WatchmodeClient::with_options("test-key", server.uri(), Duration::from_secs(60))
                .unwrap();
        let offers = client
            .offers_for_tmdb_id(42, MediaKind::Movie, "es")
            .await
            .unwrap();
        assert_eq!(offers.len(), 3);
        assert_eq!(offers[0].service_id, "203");
        assert_eq!(offers[0].offer_type, OfferType::Subscription);

        let configured = filter_subscription_offers(
            offers,
            &[StreamingService::Netflix, StreamingService::PrimeVideo],
        );
        assert_eq!(configured.len(), 2);
    }

    #[tokio::test]
    async fn rejects_invalid_regions_and_handles_rate_limits() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/title/1/sources/"))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "30"))
            .mount(&server)
            .await;
        let client = WatchmodeClient::new("test-key").unwrap();
        assert!(matches!(
            client
                .sources_for_title(1, crate::MediaItemId::new(), "Spain")
                .await,
            Err(WatchmodeError::InvalidResponse(_))
        ));

        let client =
            WatchmodeClient::with_options("test-key", server.uri(), Duration::from_secs(0))
                .unwrap();
        assert!(matches!(
            client
                .sources_for_title(1, crate::MediaItemId::new(), "ES")
                .await,
            Err(WatchmodeError::RateLimited {
                retry_after_seconds: Some(30)
            })
        ));
    }
}
