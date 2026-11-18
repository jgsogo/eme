//! Retrieve public X/Twitter posts through the FxEmbed API.

use std::time::Duration;

use agent_runtime::{Tool, ToolContext, ToolError};
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde_json::Value;
use url::Url;

const API_BASE_URL: &str = "https://api.fxtwitter.com";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const TOOL_NAME: &str = "get_tweet";

/// Retrieve a tweet/post by URL. The API host is fixed; the supplied URL is
/// parsed locally and is never fetched, preventing arbitrary URL requests.
pub struct GetTweet;

#[derive(Debug, PartialEq, Eq)]
struct TweetReference {
    id: String,
}

fn parse_tweet_url(value: &str) -> Result<TweetReference, String> {
    let url = Url::parse(value).map_err(|error| format!("invalid tweet URL: {error}"))?;

    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return Err("tweet URL must use HTTPS and must not contain credentials".into());
    }

    let host = url
        .host_str()
        .ok_or_else(|| "tweet URL must contain a hostname".to_owned())?
        .to_ascii_lowercase();

    if !matches!(
        host.as_str(),
        "x.com" | "www.x.com" | "twitter.com" | "www.twitter.com"
            | "fixupx.com" | "www.fixupx.com" | "fxtwitter.com" | "www.fxtwitter.com"
    ) {
        return Err(format!("unsupported tweet URL host '{host}'"));
    }

    if url.port().is_some() {
        return Err("tweet URL must not specify a port".into());
    }

    let segments: Vec<_> = url
        .path_segments()
        .ok_or_else(|| "tweet URL must contain a post path".to_owned())?
        .filter(|segment| !segment.is_empty())
        .collect();

    if segments.len() != 3 || segments[1] != "status" {
        return Err("expected a URL path in the form /{user}/status/{tweet_id}".into());
    }

    let id = segments[2];
    if id.is_empty() || id.len() > 20 || !id.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("tweet ID must contain between 1 and 20 decimal digits".into());
    }

    Ok(TweetReference { id: id.to_owned() })
}

fn client() -> Result<Client, ToolError> {
    Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(concat!("eme-ai-platform/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| ToolError::Execution(format!("cannot create HTTP client: {error}")))
}

async fn fetch_tweet(client: &Client, api_base_url: &str, id: &str) -> Result<Value, ToolError> {
    let endpoint = format!(
        "{}/2/status/{}",
        api_base_url.trim_end_matches('/'),
        id
    );
    let response = client
        .get(endpoint)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|error| ToolError::Execution(format!("FxEmbed API request failed: {error}")))?;

    let status = response.status();
    let body = response.text().await.map_err(|error| {
        ToolError::Execution(format!("cannot read FxEmbed API response: {error}"))
    })?;

    if !status.is_success() {
        let detail = error_detail(&body);
        return Err(ToolError::Execution(format!(
            "FxEmbed API returned HTTP {status}: {detail}"
        )));
    }

    serde_json::from_str(&body).map_err(|error| {
        ToolError::Execution(format!("invalid FxEmbed API response: {error}"))
    })
}

fn error_detail(body: &str) -> String {
    let detail = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("message")
                .or_else(|| value.get("error"))
                .or_else(|| value.get("status"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| body.chars().take(512).collect());

    if detail.is_empty() {
        "empty response body".into()
    } else {
        detail
    }
}

#[async_trait]
impl Tool for GetTweet {
    async fn execute(
        &self,
        _context: &ToolContext,
        arguments: Value,
    ) -> Result<String, ToolError> {
        let url = arguments
            .get("url")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| ToolError::InvalidArguments {
                tool_name: TOOL_NAME.into(),
                errors: vec!["'url' must be a non-empty string".into()],
            })?;

        let reference = parse_tweet_url(url).map_err(|error| ToolError::InvalidArguments {
            tool_name: TOOL_NAME.into(),
            errors: vec![error],
        })?;

        let response = fetch_tweet(&client()?, API_BASE_URL, &reference.id).await?;
        serde_json::to_string_pretty(&response)
            .map_err(|error| ToolError::Execution(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path},
    };

    #[test]
    fn extracts_tweet_id_and_ignores_tracking_query_parameters() {
        let reference = parse_tweet_url(
            "https://x.com/tunmise7j/status/2123073552962216102?a=nzdW_iX5kN9G6DTiE4I&s=19",
        )
        .unwrap();

        assert_eq!(reference.id, "2123073552962216102");
    }

    #[test]
    fn accepts_twitter_and_fixupx_urls() {
        assert_eq!(
            parse_tweet_url("https://twitter.com/user/status/123").unwrap().id,
            "123"
        );
        assert_eq!(
            parse_tweet_url("https://fixupx.com/user/status/123").unwrap().id,
            "123"
        );
    }

    #[test]
    fn rejects_untrusted_hosts_and_malformed_paths() {
        assert!(parse_tweet_url("https://example.com/user/status/123").is_err());
        assert!(parse_tweet_url("https://x.com/user/status/not-a-number").is_err());
        assert!(parse_tweet_url("https://x.com/user/status/123/extra").is_err());
        assert!(parse_tweet_url("http://x.com/user/status/123").is_err());
    }

    #[tokio::test]
    async fn requests_fxembed_status_endpoint_and_preserves_response() {
        let server = MockServer::start().await;
        let payload = json!({
            "code": 200,
            "status": "ok",
            "id": "2123073552962216102",
            "url": "https://x.com/tunmise7j/status/2123073552962216102",
            "text": "A public post",
            "author": { "name": "Example", "screen_name": "tunmise7j" },
            "media": { "all": [] }
        });

        Mock::given(method("GET"))
            .and(path("/2/status/2123073552962216102"))
            .and(header("accept", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(payload.clone()))
            .mount(&server)
            .await;

        let response = fetch_tweet(&client().unwrap(), &server.uri(), "2123073552962216102")
            .await
            .unwrap();

        assert_eq!(response, payload);
    }

    #[tokio::test]
    async fn reports_api_http_errors() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/2/status/123"))
            .respond_with(
                ResponseTemplate::new(StatusCode::NOT_FOUND.as_u16())
                    .set_body_json(json!({ "message": "Post not found" })),
            )
            .mount(&server)
            .await;

        let error = fetch_tweet(&client().unwrap(), &server.uri(), "123")
            .await
            .unwrap_err();

        assert!(error.to_string().contains("HTTP 404"));
        assert!(error.to_string().contains("Post not found"));
    }
}
