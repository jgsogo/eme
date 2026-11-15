mod types;
use types::*;

use reqwest::{Client, StatusCode};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use thiserror::Error;

const DEFAULT_BASE_URL: &str = "https://www.meteosource.com/api/v1/free";

impl MeteosourceClient {
    pub(crate) fn new(api_key: String) -> Self {
        Self::with_base_url(api_key, DEFAULT_BASE_URL.to_owned())
    }

    fn with_base_url(api_key: String, base_url: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url,
        }
    }

    pub(crate) async fn weather_for_location(
        &self,
        location: &str,
    ) -> Result<Value, MeteosourceError> {
        let places: Vec<Place> = self
            .get_json("find_places", &[("text", location), ("language", "en")])
            .await?;

        let place = places
            .into_iter()
            .next()
            .ok_or_else(|| MeteosourceError::PlaceNotFound(location.to_owned()))?;

        let forecast: Value = self
            .get_json(
                "point",
                &[
                    ("place_id", place.place_id.as_str()),
                    ("sections", "current,hourly"),
                    ("language", "en"),
                    ("units", "metric"),
                ],
            )
            .await?;

        Ok(json!({
            "location": {
                "name": place.name,
                "country": place.country,
                "place_id": place.place_id,
                "timezone": place.timezone,
            },
            "weather": forecast,
        }))
    }

    async fn get_json<T: DeserializeOwned>(
        &self,
        endpoint: &str,
        query: &[(&str, &str)],
    ) -> Result<T, MeteosourceError> {
        let url = format!("{}/{}", self.base_url.trim_end_matches('/'), endpoint);
        let response = self
            .client
            .get(url)
            .header("X-API-Key", &self.api_key)
            .query(query)
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            let detail = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|value| {
                    value
                        .get("detail")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .unwrap_or(body);
            return Err(MeteosourceError::Api { status, detail });
        }

        Ok(serde_json::from_str(&body)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path, query_param},
    };

    #[tokio::test]
    async fn resolves_location_and_returns_current_and_hourly_weather() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/find_places"))
            .and(query_param("text", "Salamanca, Spain"))
            .and(query_param("language", "en"))
            .and(header("X-API-Key", "test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
                "name": "Salamanca",
                "place_id": "salamanca",
                "country": "Spain",
                "timezone": "Europe/Madrid"
            }])))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/point"))
            .and(query_param("place_id", "salamanca"))
            .and(query_param("sections", "current,hourly"))
            .and(query_param("units", "metric"))
            .and(header("X-API-Key", "test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "current": {
                    "summary": "Clear",
                    "temperature": 18.5
                },
                "hourly": {
                    "data": []
                }
            })))
            .mount(&server)
            .await;

        let client = MeteosourceClient::with_base_url("test-key".into(), server.uri());
        let result = client
            .weather_for_location("Salamanca, Spain")
            .await
            .unwrap();

        assert_eq!(result["location"]["place_id"], "salamanca");
        assert_eq!(result["weather"]["current"]["temperature"], 18.5);
    }

    #[tokio::test]
    async fn reports_when_no_place_matches() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/find_places"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&server)
            .await;

        let client = MeteosourceClient::with_base_url("test-key".into(), server.uri());
        let error = client.weather_for_location("Nowhere").await.unwrap_err();

        assert!(matches!(error, MeteosourceError::PlaceNotFound(_)));
    }

    #[tokio::test]
    async fn surfaces_api_error_details() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/find_places"))
            .respond_with(ResponseTemplate::new(429).set_body_json(json!({
                "detail": "Rate limit exceeded"
            })))
            .mount(&server)
            .await;

        let client = MeteosourceClient::with_base_url("test-key".into(), server.uri());
        let error = client.weather_for_location("Salamanca").await.unwrap_err();

        assert!(error.to_string().contains("Rate limit exceeded"));
    }
}
