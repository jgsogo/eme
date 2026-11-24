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

    /// Create a client scoped to one library selector value returned by RABEL.
    pub fn for_subcatalog(subcatalog: &str) -> Result<Self, RabelError> {
        if subcatalog.trim().is_empty() {
            return Err(RabelError::Parse("subcatalog must not be empty".into()));
        }
        Self::with_endpoint(Url::parse(OPAC_URL).expect("valid OPAC URL"), subcatalog)
            .map_err(RabelError::Request)
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
        // Start at the public OPAC URL to establish an AbsysNet session. The
        // bootstrap response redirects/meta-refreshes to a session root; the
        // actual form action on that page is the session's /NT1 endpoint.
        let response = self.http.get(self.opac_url.clone()).send().await?;
        if !response.status().is_success() {
            return Err(RabelError::Http(response.status()));
        }
        let bootstrap_url = response.url().clone();
        let bootstrap_html = response.text().await?;
        let target = parser::meta_refresh_target(&bootstrap_html)
            .ok_or_else(|| RabelError::Parse("search bootstrap did not contain a session URL".into()))?;
        let session_root = bootstrap_url.join(&target)
            .map_err(|error| RabelError::Parse(format!("invalid session URL: {error}")))?;
        let (landing_url, landing_html) = self.read_html(session_root).await?;

        // AbsysNet serves the catalogue shell from the session root. Its
        // forms post to /NT1, but direct catalogue searches are supported as
        // GET requests using ACC=DOSEARCH and xsqf01 (Baratz OPAC URL-search
        // convention). Sending the query to the root only returns the shell.
        let mut search_url = parser::session_action(&landing_html, &landing_url)
            .ok_or_else(|| RabelError::Parse(format!(
                "RABEL landing page did not expose a session form action; {}",
                parser::search_page_diagnostic(&landing_html, &landing_url)
            )))?;
        search_url.query_pairs_mut()
            .append_pair("SUBC", &self.subcatalog)
            .append_pair("ACC", "DOSEARCH")
            .append_pair("xsqf01", query);
        self.read_html(search_url).await
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<CatalogueSearchResult>, RabelError> {
        if query.trim().is_empty() {
            return Err(RabelError::Parse("query must not be empty".into()));
        }
        if !(1..=20).contains(&limit) {
            return Err(RabelError::Parse("limit must be between 1 and 20".into()));
        }

        // AbsysNet can interpret a multi-word title as an overly restrictive
        // phrase, and some query forms occasionally fail upstream. Retry with
        // meaningful title terms before returning an empty result or an error.
        let fallback = fallback_search_query(query);
        match self.search_once(query).await {
            Ok(results) if !results.is_empty() => Ok(results.into_iter().take(limit).collect()),
            Ok(_) => match fallback {
                Some(fallback) => self.search_once(&fallback).await
                    .map(|results| results.into_iter().take(limit).collect()),
                None => Ok(Vec::new()),
            },
            Err(original_error) => {
                if let Some(fallback) = fallback {
                    if let Ok(results) = self.search_once(&fallback).await {
                        if !results.is_empty() {
                            return Ok(results.into_iter().take(limit).collect());
                        }
                    }
                }
                Err(original_error)
            }
        }
    }

    async fn search_once(&self, query: &str) -> Result<Vec<CatalogueSearchResult>, RabelError> {
        let (session_url, html) = self.search_page(query).await?;
        parser::search_results(&html, &session_url)
    }

    pub async fn get_item(&self, record_ref: &str) -> Result<CatalogueItem, RabelError> {
        let url = self.validate_record_ref(record_ref)?;
        let (final_url, html) = self.read_html(url).await?;
        parser::record(&html, final_url.as_str())
    }

    pub async fn libraries(&self) -> Result<Vec<Library>, RabelError> {
        // Like catalogue searches, the OPAC's first response is a bootstrap
        // page containing a session-specific URL. The library selector is
        // rendered on that session page, not reliably on the bootstrap page.
        let mut url = self.opac_url.clone();
        url.query_pairs_mut().append_pair("SUBC", &self.subcatalog);
        let response = self.http.get(url).send().await?;
        if !response.status().is_success() {
            return Err(RabelError::Http(response.status()));
        }
        let bootstrap_url = response.url().clone();
        let bootstrap_html = response.text().await?;

        // The library selector belongs to the OPAC's landing page in some
        // RABEL configurations. Unlike a search, listing libraries must not
        // assume that the selector is present after following the session
        // meta-refresh (the session page may already be scoped to one library).
        if let Ok(libraries) = parser::libraries(&bootstrap_html) {
            return Ok(libraries);
        }

        let target = parser::meta_refresh_target(&bootstrap_html)
            .ok_or_else(|| RabelError::Parse("library selector not found in RABEL bootstrap page; no session URL to inspect".into()))?;
        let session_url = bootstrap_url.join(&target)
            .map_err(|error| RabelError::Parse(format!("invalid session URL: {error}")))?;
        let (_, html) = self.read_html(session_url).await?;
        parser::libraries(&html)
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


fn fallback_search_query(query: &str) -> Option<String> {
    const STOP_WORDS: &[&str] = &[
        "a", "al", "con", "como", "de", "del", "el", "en", "la", "las",
        "lo", "los", "para", "por", "un", "una", "unas", "unos", "y",
    ];

    let terms = query.split(|character: char| !character.is_alphanumeric())
        .filter(|term| !term.is_empty())
        .filter(|term| !STOP_WORDS.contains(&term.to_lowercase().as_str()))
        .collect::<Vec<_>>();
    (terms.len() >= 2).then(|| terms.join(" "))
}

#[cfg(test)]
mod search_query_tests {
    use super::fallback_search_query;

    #[test]
    fn removes_spanish_stop_words_for_fallback_search() {
        assert_eq!(fallback_search_query("La trampa de la diversidad").as_deref(), Some("trampa diversidad"));
    }

    #[test]
    fn does_not_create_a_fallback_for_a_single_meaningful_term() {
        assert_eq!(fallback_search_query("La diversidad"), None);
    }
}
