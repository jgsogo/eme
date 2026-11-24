//! HTTP and session handling for the Salamanca RABEL OPAC.

use std::time::Duration;

use reqwest::{Client, StatusCode};
use thiserror::Error;
use url::Url;

use crate::{parser, types::{CatalogueItem, CatalogueSearchResult, Library}};

const OPAC_URL: &str = "https://rabel.jcyl.es/cgi-bin/abnetopac2";
const SUBCATALOG: &str = "SAL/BPSA";
const MAX_RESPONSE_BYTES: usize = 5 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum RabelError {
    #[error("RABEL request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("RABEL returned HTTP {0}")]
    Http(StatusCode),
    #[error("unexpected RABEL response: {0}")]
    Parse(String),
    #[error("invalid record reference")]
    InvalidRecordReference,
}

/// Stateful client: one reqwest client owns the cookie jar used by AbsysNet.
pub struct RabelClient {
    http: Client,
    opac_url: Url,
    subcatalog: String,
}

impl RabelClient {
    pub fn new() -> Result<Self, reqwest::Error> {
        Self::with_endpoint(Url::parse(OPAC_URL).expect("valid OPAC URL"), SUBCATALOG)
    }

    fn with_endpoint(opac_url: Url, subcatalog: &str) -> Result<Self, reqwest::Error> {
        let http = Client::builder()
            .cookie_store(true)
            .timeout(Duration::from_secs(20))
            .user_agent(concat!("eme-rabel/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self { http, opac_url, subcatalog: subcatalog.to_owned() })
    }

    async fn read_html(&self, url: Url) -> Result<(Url, String), RabelError> {
        let response = self.http.get(url).send().await?;
        if !response.status().is_success() {
            return Err(RabelError::Http(response.status()));
        }
        let final_url = response.url().clone();
        // RABEL currently declares ISO-8859-1. reqwest's text() handles
        // charset headers; use it for correct Spanish accents.
        let html = response.text().await?;
        if html.len() > MAX_RESPONSE_BYTES {
            return Err(RabelError::Parse("response exceeds 5 MiB limit".into()));
        }
        Ok((final_url, html))
    }

    async fn search_page(&self, query: &str) -> Result<(Url, String), RabelError> {
        let mut url = self.opac_url.clone();
        url.query_pairs_mut()
            .append_pair("SUBC", &self.subcatalog)
            .append_pair("ACC", "DOSEARCH")
            .append_pair("xsqf01", query);
        let response = self.http.get(url).send().await?;
        if !response.status().is_success() {
            return Err(RabelError::Http(response.status()));
        }
        let bootstrap_url = response.url().clone();
        let html = response.text().await?;
        let target = parser::meta_refresh_target(&html)
            .ok_or_else(|| RabelError::Parse("search bootstrap did not contain a session URL".into()))?;
        let session_url = bootstrap_url.join(&target)
            .map_err(|error| RabelError::Parse(format!("invalid session URL: {error}")))?;
        self.read_html(session_url).await
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<CatalogueSearchResult>, RabelError> {
        if query.trim().is_empty() {
            return Err(RabelError::Parse("query must not be empty".into()));
        }
        if !(1..=20).contains(&limit) {
            return Err(RabelError::Parse("limit must be between 1 and 20".into()));
        }
        let (session_url, html) = self.search_page(query).await?;
        let results = parser::search_results(&html, &session_url)?;
        Ok(results.into_iter().take(limit).collect())
    }

    pub async fn get_item(&self, record_ref: &str) -> Result<CatalogueItem, RabelError> {
        let url = self.validate_record_ref(record_ref)?;
        let (final_url, html) = self.read_html(url).await?;
        parser::record(&html, final_url.as_str())
    }

    pub async fn libraries(&self) -> Result<Vec<Library>, RabelError> {
        let mut url = self.opac_url.clone();
        url.query_pairs_mut().append_pair("SUBC", &self.subcatalog);
        let (_, html) = self.read_html(url).await?;
        Ok(parser::libraries(&html))
    }

    fn validate_record_ref(&self, value: &str) -> Result<Url, RabelError> {
        let url = self.opac_url.join(value).map_err(|_| RabelError::InvalidRecordReference)?;
        if url.scheme() != "https"
            || url.host_str() != self.opac_url.host_str()
            || !url.path().starts_with("/cgi-bin/abnetopac2/")
            || url.query_pairs().all(|(key, _)| key != "ACC")
        {
            return Err(RabelError::InvalidRecordReference);
        }
        Ok(url)
    }
}
