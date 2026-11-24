//! HTTP access to the Salamanca Public Library OPAC.

use std::time::Duration;

use reqwest::Client;
use url::Url;

use crate::SearchResponse;

const OPAC_URL: &str = "https://rabel.jcyl.es/cgi-bin/abnetopac2";
const SUBCATALOG: &str = "SAL/BPSA";

/// Minimal client for probing the Salamanca AbsysNet OPAC.
pub struct RabelClient {
    http: Client,
    opac_url: Url,
    subcatalog: String,
}

impl RabelClient {
    /// Create a client targeting the Casa de las Conchas subcatalogue.
    pub fn new() -> Result<Self, reqwest::Error> {
        let opac_url = Url::parse(OPAC_URL).expect("the configured OPAC URL is valid");
        Self::with_endpoint(opac_url, SUBCATALOG)
    }

    fn with_endpoint(opac_url: Url, subcatalog: &str) -> Result<Self, reqwest::Error> {
        let http = Client::builder()
            .cookie_store(true)
            .timeout(Duration::from_secs(20))
            .user_agent(concat!("eme-rabel-poc/", env!("CARGO_PKG_VERSION")))
            .build()?;

        Ok(Self {
            http,
            opac_url,
            subcatalog: subcatalog.to_owned(),
        })
    }

    /// Submit an all-fields search and return the response without interpreting
    /// its HTML. The field index is the standard AbsysNet all-fields index and
    /// still needs to be verified against the live RABEL installation.
    pub async fn search(&self, query: &str) -> Result<SearchResponse, reqwest::Error> {
        let mut url = self.opac_url.clone();
        url.query_pairs_mut()
            .append_pair("SUBC", &self.subcatalog)
            .append_pair("ACC", "DOSEARCH")
            .append_pair("xsqf01", query);

        let response = self.http.get(url).send().await?;
        let status = response.status().as_u16();
        let final_url = response.url().to_string();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let html = response.text().await?;

        Ok(SearchResponse {
            status,
            final_url,
            content_type,
            html,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path, query_param},
    };

    #[tokio::test]
    async fn submits_encoded_search_to_the_configured_subcatalogue() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/cgi-bin/abnetopac2"))
            .and(query_param("SUBC", "SAL/BPSA"))
            .and(query_param("ACC", "DOSEARCH"))
            .and(query_param("xsqf01", "café & Rust"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html; charset=utf-8")
                    .set_body_string("<html><title>Results</title></html>"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = RabelClient::with_endpoint(
            Url::parse(&format!("{}/cgi-bin/abnetopac2", server.uri())).unwrap(),
            SUBCATALOG,
        )
        .unwrap();

        let response = client.search("café & Rust").await.unwrap();

        assert_eq!(response.status, 200);
        assert_eq!(
            response.content_type.as_deref(),
            Some("text/html; charset=utf-8")
        );
        assert!(response.html.contains("<title>Results</title>"));
        assert!(response.final_url.contains("ACC=DOSEARCH"));
    }

    #[tokio::test]
    async fn preserves_non_success_http_status_for_diagnostics() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503).set_body_string("temporarily unavailable"))
            .mount(&server)
            .await;

        let client = RabelClient::with_endpoint(
            Url::parse(&format!("{}/cgi-bin/abnetopac2", server.uri())).unwrap(),
            SUBCATALOG,
        )
        .unwrap();

        let response = client.search("test").await.unwrap();

        assert_eq!(response.status, 503);
        assert_eq!(response.html, "temporarily unavailable");
    }
}
