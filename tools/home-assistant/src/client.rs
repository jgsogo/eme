//! Authenticated HTTP client for the Home Assistant REST API.

use std::time::Duration;

use reqwest::{Client, Method, StatusCode};
use serde_json::Value;
use url::Url;

use crate::error::HomeAssistantError;

/// Minimal Home Assistant REST API client.
pub struct HomeAssistantClient {
    client: Client,
    base_url: Url,
}

impl HomeAssistantClient {
    /// Build a client from a base URL.
    pub fn new(base_url: &str) -> Result<Self, HomeAssistantError> {
        let mut base_url =
            Url::parse(base_url).map_err(|error| HomeAssistantError::InvalidUrl(error.to_string()))?;

        if !matches!(base_url.scheme(), "http" | "https")
            || base_url.host_str().is_none()
            || !base_url.username().is_empty()
            || base_url.password().is_some()
            || base_url.query().is_some()
            || base_url.fragment().is_some()
        {
            return Err(HomeAssistantError::InvalidUrl(
                "expected an http(s) URL without embedded credentials, query, or fragment".into(),
            ));
        }

        if !base_url.path().ends_with('/') {
            let path = format!("{}/", base_url.path());
            base_url.set_path(&path);
        }

        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

        Ok(Self { client, base_url })
    }

    fn endpoint(&self, path: &str) -> Result<Url, HomeAssistantError> {
        self.base_url
            .join(path)
            .map_err(|error| HomeAssistantError::InvalidUrl(error.to_string()))
    }

    async fn request(
        &self,
        method: Method,
        url: Url,
        token: &str,
    ) -> Result<Value, HomeAssistantError> {
        let response = self
            .client
            .request(method, url)
            .bearer_auth(token)
            .send()
            .await?;
        self.parse_response(response).await
    }

    async fn parse_response(
        &self,
        response: reqwest::Response,
    ) -> Result<Value, HomeAssistantError> {
        if !response.status().is_success() {
            let status: StatusCode = response.status();
            let body = response.text().await.unwrap_or_default();
            let message = body.chars().take(512).collect::<String>();
            return Err(HomeAssistantError::Http {
                status: status.as_u16(),
                message: if message.is_empty() {
                    status.canonical_reason().unwrap_or("request failed").to_owned()
                } else {
                    message
                },
            });
        }
        Ok(response.json().await?)
    }

    /// Return all current entity states.
    pub async fn get_states(&self, token: &str) -> Result<Value, HomeAssistantError> {
        let url = self.endpoint("api/states")?;
        self.request(Method::GET, url, token).await
    }

    /// Return the current state for one entity.
    pub async fn get_state(
        &self,
        token: &str,
        entity_id: &str,
    ) -> Result<Value, HomeAssistantError> {
        let mut url = self.endpoint("api/states/")?;
        url.path_segments_mut()
            .map_err(|_| HomeAssistantError::InvalidUrl("cannot append entity ID".into()))?
            .pop_if_empty()
            .push(entity_id);
        self.request(Method::GET, url, token).await
    }

    /// Discover domains and services exposed by Home Assistant.
    pub async fn get_services(&self, token: &str) -> Result<Value, HomeAssistantError> {
        let url = self.endpoint("api/services")?;
        self.request(Method::GET, url, token).await
    }

    /// Invoke a Home Assistant service.
    pub async fn call_service(
        &self,
        token: &str,
        domain: &str,
        service: &str,
        service_data: Value,
    ) -> Result<Value, HomeAssistantError> {
        let mut url = self.endpoint("api/services/")?;
        url.path_segments_mut()
            .map_err(|_| HomeAssistantError::InvalidUrl("cannot append service path".into()))?
            .pop_if_empty()
            .push(domain)
            .push(service);

        let response = self
            .client
            .post(url)
            .bearer_auth(token)
            .json(&service_data)
            .send()
            .await?;
        self.parse_response(response).await
    }

    /// Return state history, optionally filtered to one entity.
    pub async fn get_history(
        &self,
        token: &str,
        entity_id: Option<&str>,
        timestamp: Option<&str>,
    ) -> Result<Value, HomeAssistantError> {
        let mut url = self.endpoint("api/history/period")?;
        if let Some(timestamp) = timestamp {
            url.path_segments_mut()
                .map_err(|_| HomeAssistantError::InvalidUrl("cannot append timestamp".into()))?
                .pop_if_empty()
                .push(timestamp);
        }
        if let Some(entity_id) = entity_id {
            url.query_pairs_mut()
                .append_pair("filter_entity_id", entity_id);
        }
        self.request(Method::GET, url, token).await
    }

    /// Return Home Assistant's core configuration.
    pub async fn get_config(&self, token: &str) -> Result<Value, HomeAssistantError> {
        let url = self.endpoint("api/config")?;
        self.request(Method::GET, url, token).await
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_json, header, method, path, query_param},
    };

    use super::HomeAssistantClient;

    #[tokio::test]
    async fn gets_state_with_bearer_auth() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/states/light.kitchen"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "entity_id": "light.kitchen",
                "state": "on"
            })))
            .mount(&server)
            .await;

        let client = HomeAssistantClient::new(&server.uri()).unwrap();
        let state = client.get_state("test-token", "light.kitchen").await.unwrap();
        assert_eq!(state["state"], "on");
    }

    #[tokio::test]
    async fn gets_history_with_entity_filter() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/history/period"))
            .and(query_param("filter_entity_id", "sensor.temperature"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&server)
            .await;

        let client = HomeAssistantClient::new(&server.uri()).unwrap();
        let history = client
            .get_history("test-token", Some("sensor.temperature"), None)
            .await
            .unwrap();
        assert_eq!(history, json!([]));
    }

    #[tokio::test]
    async fn calls_service_using_post_and_service_payload() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/services/light/turn_on"))
            .and(header("authorization", "Bearer test-token"))
            .and(body_json(json!({"entity_id": "light.kitchen"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&server)
            .await;

        let client = HomeAssistantClient::new(&server.uri()).unwrap();
        let result = client
            .call_service("test-token", "light", "turn_on", json!({"entity_id": "light.kitchen"}))
            .await
            .unwrap();
        assert_eq!(result, json!([]));
    }

    #[test]
    fn rejects_urls_with_credentials_or_non_http_schemes() {
        assert!(HomeAssistantClient::new("ftp://localhost").is_err());
        assert!(HomeAssistantClient::new("http://user:pass@localhost").is_err());
        assert!(HomeAssistantClient::new("http://localhost/?token=secret").is_err());
    }
}
