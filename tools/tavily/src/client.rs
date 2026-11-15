#[path = "types/mod.rs"]
mod types;
pub(crate) use types::{SearchRequest, SearchResponse, TavilyClient, TavilyError};

use std::time::Duration;

use reqwest::Client;

const DEFAULT_BASE_URL: &str = "https://api.tavily.com";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

impl TavilyClient {
    pub(crate) fn new(api_key: String) -> Result<Self, TavilyError> {
        Self::with_base_url(api_key, DEFAULT_BASE_URL.to_owned())
    }

    fn with_base_url(api_key: String, base_url: String) -> Result<Self, TavilyError> {
        let client = Client::builder().timeout(REQUEST_TIMEOUT).build()?;
        Ok(Self {
            client,
            api_key,
            base_url,
        })
    }

    pub(crate) async fn search(
        &self,
        request: &SearchRequest,
    ) -> Result<SearchResponse, TavilyError> {
        let response = self
            .client
            .post(format!("{}/search", self.base_url.trim_end_matches('/')))
            .bearer_auth(&self.api_key)
            .json(request)
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            let detail = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|value| {
                    let detail = value.get("detail").unwrap_or(&value);
                    detail
                        .as_str()
                        .or_else(|| detail.get("error").and_then(serde_json::Value::as_str))
                        .or_else(|| detail.get("message").and_then(serde_json::Value::as_str))
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| {
                    if body.len() > 512 {
                        format!("{}…", &body[..512])
                    } else {
                        body
                    }
                });
            return Err(TavilyError::Api { status, detail });
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
        matchers::{body_json, header, method, path},
    };

    fn request() -> SearchRequest {
        SearchRequest {
            query: "Rust agent frameworks".into(),
            search_depth: "basic".into(),
            topic: "general".into(),
            max_results: 5,
            time_range: None,
            include_domains: None,
            exclude_domains: None,
        }
    }

    #[tokio::test]
    async fn sends_authenticated_search_and_deserializes_results() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/search"))
            .and(header("authorization", "Bearer test-key"))
            .and(body_json(json!({
                "query": "Rust agent frameworks",
                "search_depth": "basic",
                "topic": "general",
                "max_results": 5
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "query": "Rust agent frameworks",
                "response_time": 1.23,
                "results": [{
                    "title": "An article",
                    "url": "https://example.com/article",
                    "content": "A useful summary.",
                    "score": 0.92,
                    "published_date": "2026-11-10",
                    "raw_content": "Ignored by the response model"
                }]
            })))
            .mount(&server)
            .await;

        let client = TavilyClient::with_base_url("test-key".into(), server.uri()).unwrap();
        let response = client.search(&request()).await.unwrap();

        assert_eq!(response.results.len(), 1);
        assert_eq!(response.results[0].title, "An article");
        assert_eq!(response.results[0].score, Some(0.92));
        assert_eq!(response.response_time, Some(1.23));
    }

    #[tokio::test]
    async fn includes_optional_filters_in_the_request() {
        let server = MockServer::start().await;
        let mut request = request();
        request.time_range = Some("week".into());
        request.include_domains = Some(vec!["rust-lang.org".into()]);
        request.exclude_domains = Some(vec!["example.org".into()]);

        Mock::given(method("POST"))
            .and(path("/search"))
            .and(body_json(json!({
                "query": "Rust agent frameworks",
                "search_depth": "basic",
                "topic": "general",
                "max_results": 5,
                "time_range": "week",
                "include_domains": ["rust-lang.org"],
                "exclude_domains": ["example.org"]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "query": "Rust agent frameworks",
                "results": []
            })))
            .mount(&server)
            .await;

        let client = TavilyClient::with_base_url("test-key".into(), server.uri()).unwrap();
        let response = client.search(&request).await.unwrap();

        assert!(response.results.is_empty());
    }

    #[tokio::test]
    async fn reports_api_error_details() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/search"))
            .respond_with(ResponseTemplate::new(429).set_body_json(json!({
                "detail": "Rate limit exceeded"
            })))
            .mount(&server)
            .await;

        let client = TavilyClient::with_base_url("test-key".into(), server.uri()).unwrap();
        let error = client.search(&request()).await.unwrap_err();

        assert!(error.to_string().contains("Rate limit exceeded"));
    }
}
