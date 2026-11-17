//! HTTP client for the anonymous Mercadona catalogue and Algolia search APIs.

use reqwest::{Client, Method, Response, header};
use serde_json::{Value, json};
use std::time::Duration;
use url::form_urlencoded;

const BASE_URL: &str = "https://tienda.mercadona.es";
const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";
const API_VERSION: &str = "v9200";
const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

/// Client for public Mercadona catalogue endpoints and the public Algolia search index.
pub struct MercadonaClient {
    http: Client,
    warehouse: String,
    language: String,
}

impl MercadonaClient {
    /// Create a client for one warehouse and language.
    pub fn new(warehouse: impl Into<String>) -> Result<Self, String> {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(USER_AGENT)
            .build()
            .map_err(|error| format!("cannot create Mercadona HTTP client: {error}"))?;
        let warehouse = warehouse.into();
        if warehouse.is_empty()
            || !warehouse
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
        {
            return Err("warehouse must contain only letters, digits, '_' or '-'".to_owned());
        }

        let language = std::env::var("MERCADONA_LANG")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "es".to_owned());
        if !language
            .bytes()
            .all(|byte| byte.is_ascii_alphabetic() || byte == b'-')
        {
            return Err("MERCADONA_LANG must contain only ASCII letters or '-'".to_owned());
        }

        Ok(Self {
            http,
            warehouse,
            language,
        })
    }

    fn api_url(&self, path: &str) -> String {
        format!("{BASE_URL}{path}")
    }

    async fn send_api(
        &self,
        method: Method,
        url: String,
        body: Option<Value>,
    ) -> Result<Value, String> {
        let mut request = self
            .http
            .request(method, url)
            .header(header::ACCEPT, "application/json")
            .header(header::ACCEPT_LANGUAGE, "es-ES,es;q=0.9,en;q=0.8")
            .header(header::REFERER, format!("{BASE_URL}/"))
            .header(header::ORIGIN, BASE_URL)
            .header("x-version", API_VERSION)
            .header(
                "x-customer-device-id",
                "00000000-0000-0000-0000-000000000000",
            );

        if let Some(body) = body {
            request = request.json(&body);
        }

        let response = request
            .send()
            .await
            .map_err(|error| format!("Mercadona API request failed: {error}"))?;
        Self::decode_response(response).await
    }

    async fn decode_response(response: Response) -> Result<Value, String> {
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|error| format!("cannot read Mercadona API response: {error}"))?;
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err("Mercadona API response exceeded the 16 MiB limit".to_owned());
        }
        if !status.is_success() {
            let body = String::from_utf8_lossy(&bytes);
            return Err(format!(
                "Mercadona API returned HTTP {status}: {}",
                body.chars().take(1000).collect::<String>()
            ));
        }
        serde_json::from_slice(&bytes)
            .map_err(|error| format!("invalid JSON returned by Mercadona API: {error}"))
    }

    fn catalogue_url(&self, path: &str) -> String {
        let mut query = form_urlencoded::Serializer::new(String::new());
        query.append_pair("lang", &self.language);
        query.append_pair("wh", &self.warehouse);
        format!("{BASE_URL}{path}?{}", query.finish())
    }

    /// Search the warehouse's Algolia product index.
    pub async fn search_products(&self, query: &str, limit: usize) -> Result<Value, String> {
        let (app_id, api_key) = self.algolia_credentials().await?;
        let index = format!("products_prod_{}_{}", self.warehouse, self.language);
        let body = {
            let mut params = form_urlencoded::Serializer::new(String::new());
            params.append_pair("query", query);
            params.append_pair("hitsPerPage", &limit.to_string());
            json!({ "params": params.finish() })
        };
        let url = format!(
            "https://{}-dsn.algolia.net/1/indexes/{index}/query",
            app_id.to_lowercase()
        );

        let response = self
            .http
            .post(url)
            .header("X-Algolia-API-Key", api_key)
            .header("X-Algolia-Application-Id", app_id)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::REFERER, format!("{BASE_URL}/"))
            .json(&body)
            .send()
            .await
            .map_err(|error| format!("Algolia search request failed: {error}"))?;
        Self::decode_response(response).await
    }

    async fn algolia_credentials(&self) -> Result<(String, String), String> {
        if let (Ok(app_id), Ok(api_key)) = (
            std::env::var("MERCADONA_ALGOLIA_APP_ID"),
            std::env::var("MERCADONA_ALGOLIA_API_KEY"),
        ) {
            if !app_id.trim().is_empty() && !api_key.trim().is_empty() {
                return Ok((app_id, api_key));
            }
        }

        let shell = self
            .http
            .get(format!("{BASE_URL}/"))
            .header(header::ACCEPT, "text/html")
            .send()
            .await
            .map_err(|error| format!("cannot discover Algolia credentials: {error}"))?
            .error_for_status()
            .map_err(|error| format!("Mercadona SPA shell request failed: {error}"))?
            .text()
            .await
            .map_err(|error| format!("cannot read Mercadona SPA shell: {error}"))?;

        let bundle_path = Self::find_bundle_path(&shell)
            .ok_or_else(|| "Mercadona SPA entry bundle was not found".to_owned())?;
        let bundle = self
            .http
            .get(format!("{BASE_URL}{bundle_path}"))
            .send()
            .await
            .map_err(|error| format!("cannot fetch Mercadona SPA bundle: {error}"))?
            .error_for_status()
            .map_err(|error| format!("Mercadona SPA bundle request failed: {error}"))?
            .text()
            .await
            .map_err(|error| format!("cannot read Mercadona SPA bundle: {error}"))?;

        Self::parse_algolia_credentials(&bundle).ok_or_else(|| {
            "Algolia credentials were not found in the Mercadona SPA bundle; configure MERCADONA_ALGOLIA_APP_ID and MERCADONA_ALGOLIA_API_KEY".to_owned()
        })
    }

    fn find_bundle_path(shell: &str) -> Option<String> {
        for marker in ["src=\"", "src='"] {
            let mut rest = shell;
            while let Some(index) = rest.find(marker) {
                rest = &rest[index + marker.len()..];
                let end_double = rest.find('"');
                let end_single = rest.find('\'');
                let end = match (end_double, end_single) {
                    (Some(left), Some(right)) => left.min(right),
                    (Some(value), None) | (None, Some(value)) => value,
                    (None, None) => return None,
                };
                let candidate = &rest[..end];
                if candidate.starts_with("/v")
                    && candidate.contains("/index-")
                    && candidate.ends_with(".js")
                {
                    return Some(candidate.to_owned());
                }
                rest = &rest[end + 1..];
            }
        }
        None
    }

    fn parse_algolia_credentials(bundle: &str) -> Option<(String, String)> {
        let bytes = bundle.as_bytes();
        for (start, _) in bundle.match_indices('"') {
            let app_start = start + 1;
            let app_end = app_start + 10;
            if app_end >= bytes.len() || bytes.get(app_end) != Some(&b'"') {
                continue;
            }
            let app_id = &bundle[app_start..app_end];
            if !app_id
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
            {
                continue;
            }

            let tail = &bundle[app_end + 1..];
            let Some(after_comma) = tail.strip_prefix(',') else {
                continue;
            };
            let after_comma = after_comma.trim_start();
            let Some(equal) = after_comma.find("=\"") else {
                continue;
            };
            let key_start = equal + 2;
            let key_tail = &after_comma[key_start..];
            let Some(key_end) = key_tail.find('"') else {
                continue;
            };
            let api_key = &key_tail[..key_end];
            if api_key.len() == 32 && api_key.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Some((app_id.to_owned(), api_key.to_owned()));
            }
        }
        None
    }

    /// Fetch the category tree for this warehouse.
    pub async fn categories(&self) -> Result<Value, String> {
        self.send_api(Method::GET, self.catalogue_url("/api/categories/"), None)
            .await
    }

    /// Fetch one category, including its products/subcategories.
    pub async fn category(&self, id: &str) -> Result<Value, String> {
        let id = url::form_urlencoded::byte_serialize(id.as_bytes()).collect::<String>();
        self.send_api(
            Method::GET,
            self.catalogue_url(&format!("/api/categories/{id}/")),
            None,
        )
        .await
    }

    /// Fetch one product's full detail.
    pub async fn product(&self, id: &str) -> Result<Value, String> {
        let id = url::form_urlencoded::byte_serialize(id.as_bytes()).collect::<String>();
        self.send_api(
            Method::GET,
            self.catalogue_url(&format!("/api/products/{id}/")),
            None,
        )
        .await
    }

    /// Resolve a Spanish postal code to the warehouse serving it.
    pub async fn resolve_postal_code(&self, postal_code: &str) -> Result<Value, String> {
        let url = self.api_url("/api/postal-codes/actions/change-pc/");
        let response = self
            .http
            .post(url)
            .header(header::ACCEPT, "application/json")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::REFERER, format!("{BASE_URL}/"))
            .header(header::ORIGIN, BASE_URL)
            .header("x-version", API_VERSION)
            .json(&json!({ "new_postal_code": postal_code }))
            .send()
            .await
            .map_err(|error| format!("postal-code lookup failed: {error}"))?;

        let status = response.status();
        let warehouse = response
            .headers()
            .get("x-customer-wh")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(format!(
                "Mercadona postal-code API returned HTTP {status}: {}",
                body.chars().take(1000).collect::<String>()
            ));
        }
        if warehouse.is_empty() {
            return Err(format!(
                "Mercadona did not return a warehouse for postal code {postal_code}; it may not be covered for online delivery"
            ));
        }
        Ok(json!({
            "postal_code": postal_code,
            "warehouse": warehouse,
            "message": "Use this warehouse code in the warehouse argument of catalogue tools."
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_versioned_spa_entry_bundle() {
        assert_eq!(
            MercadonaClient::find_bundle_path(
                r#"<script type="module" src="/v9200/index-AbC_123.js"></script>"#
            ),
            Some("/v9200/index-AbC_123.js".to_owned())
        );
    }

    #[test]
    fn extracts_algolia_credentials_from_bundle() {
        let bundle = r#"const x="7UZJKL1DJ0",BGe="9d8f2e39e90df472b4f2e559a116fe17";"#;
        assert_eq!(
            MercadonaClient::parse_algolia_credentials(bundle),
            Some((
                "7UZJKL1DJ0".to_owned(),
                "9d8f2e39e90df472b4f2e559a116fe17".to_owned()
            ))
        );
    }

    #[test]
    fn ignores_malformed_algolia_credentials() {
        assert_eq!(
            MercadonaClient::parse_algolia_credentials(r#"const x="not-an-app-id",BGe="short";"#),
            None
        );
    }
}
