//! HTTP and session handling for the Salamanca RABEL OPAC.

use std::collections::HashSet;
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

pub struct RabelClient {
    http: Client,
    opac_url: Url,
    subcatalog: String,
}

impl RabelClient {
    pub fn new() -> Result<Self, reqwest::Error> {
        Self::with_endpoint(Url::parse(OPAC_URL).expect("valid OPAC URL"), SUBCATALOG)
    }

    pub fn for_subcatalog(subcatalog: &str) -> Result<Self, RabelError> {
        if subcatalog.trim().is_empty() {
            return Err(RabelError::Parse("subcatalog must not be empty".into()));
        }
        Self::with_endpoint(Url::parse(OPAC_URL).expect("valid OPAC URL"), subcatalog)
            .map_err(RabelError::Request)
    }

    fn with_endpoint(opac_url: Url, subcatalog: &str) -> Result<Self, reqwest::Error> {
        let http = Client::builder().cookie_store(true).timeout(Duration::from_secs(20))
            .user_agent(concat!("eme-rabel/", env!("CARGO_PKG_VERSION"))).build()?;
        Ok(Self { http, opac_url, subcatalog: subcatalog.to_owned() })
    }

    async fn read_html(&self, url: Url) -> Result<(Url, String), RabelError> {
        let response = self.http.get(url).send().await?;
        if !response.status().is_success() { return Err(RabelError::Http(response.status())); }
        let final_url = response.url().clone();
        let html = response.text().await?;
        if html.len() > MAX_RESPONSE_BYTES { return Err(RabelError::Parse("response exceeds 5 MiB limit".into())); }
        Ok((final_url, html))
    }

    async fn search_page(&self, query: &str) -> Result<(Url, String), RabelError> {
        let response = self.http.get(self.opac_url.clone()).send().await?;
        if !response.status().is_success() { return Err(RabelError::Http(response.status())); }
        let bootstrap_url = response.url().clone();
        let bootstrap_html = response.text().await?;
        let target = parser::meta_refresh_target(&bootstrap_html)
            .ok_or_else(|| RabelError::Parse("search bootstrap did not contain a session URL".into()))?;
        let session_root = bootstrap_url.join(&target)
            .map_err(|error| RabelError::Parse(format!("invalid session URL: {error}")))?;
        let (landing_url, landing_html) = self.read_html(session_root).await?;
        let mut search_url = parser::session_action(&landing_html, &landing_url)
            .ok_or_else(|| RabelError::Parse(format!("RABEL landing page did not expose a session form action; {}",
                parser::search_page_diagnostic(&landing_html, &landing_url)))?;
        search_url.query_pairs_mut().append_pair("SUBC", &self.subcatalog)
            .append_pair("ACC", "DOSEARCH").append_pair("xsqf01", query);
        self.read_html(search_url).await
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<CatalogueSearchResult>, RabelError> {
        if query.trim().is_empty() { return Err(RabelError::Parse("query must not be empty".into())); }
        if !(1..=20).contains(&limit) { return Err(RabelError::Parse("limit must be between 1 and 20".into())); }

        let fallbacks = fallback_search_queries(query);
        match self.search_once(query).await {
            Ok(results) if !results.is_empty() => Ok(results.into_iter().take(limit).collect()),
            first_attempt => {
                let mut last_error = match first_attempt {
                    Err(error) => Some(error),
                    Ok(_) => None,
                };
                let mut combined = Vec::new();
                let mut seen = HashSet::new();

                for fallback in fallbacks {
                    match self.search_once(&fallback).await {
                        Ok(results) => {
                            for result in results {
                                if seen.insert(result.record_ref.clone()) {
                                    combined.push(result);
                                    if combined.len() >= limit { break; }
                                }
                            }
                        }
                        Err(error) => last_error = Some(error),
                    }
                    if combined.len() >= limit { break; }
                }

                if !combined.is_empty() { return Ok(combined); }
                if let Some(error) = last_error { return Err(error); }
                Ok(Vec::new())
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
        let mut url = self.opac_url.clone();
        url.query_pairs_mut().append_pair("SUBC", &self.subcatalog);
        let response = self.http.get(url).send().await?;
        if !response.status().is_success() { return Err(RabelError::Http(response.status())); }
        let bootstrap_url = response.url().clone();
        let bootstrap_html = response.text().await?;
        if let Ok(libraries) = parser::libraries(&bootstrap_html) { return Ok(libraries); }
        let target = parser::meta_refresh_target(&bootstrap_html)
            .ok_or_else(|| RabelError::Parse("library selector not found in RABEL bootstrap page; no session URL to inspect".into()))?;
        let session_url = bootstrap_url.join(&target)
            .map_err(|error| RabelError::Parse(format!("invalid session URL: {error}")))?;
        let (_, html) = self.read_html(session_url).await?;
        parser::libraries(&html)
    }

    fn validate_record_ref(&self, value: &str) -> Result<Url, RabelError> {
        let url = self.opac_url.join(value).map_err(|_| RabelError::InvalidRecordReference)?;
        if url.scheme() != "https" || url.host_str() != self.opac_url.host_str()
            || !url.path().starts_with("/cgi-bin/abnetopac2/")
            || url.query_pairs().all(|(key, _)| key != "ACC") {
            return Err(RabelError::InvalidRecordReference);
        }
        Ok(url)
    }
}

fn fallback_search_queries(query: &str) -> Vec<String> {
    const STOP_WORDS: &[&str] = &[
        "a", "al", "con", "como", "de", "del", "el", "en", "la", "las",
        "lo", "los", "para", "por", "un", "una", "unas", "unos", "y",
    ];
    let terms = query.split(|character: char| !character.is_alphanumeric())
        .filter(|term| !term.is_empty())
        .filter(|term| !STOP_WORDS.contains(&term.to_lowercase().as_str()))
        .collect::<Vec<_>>();
    let mut candidates = Vec::new();
    if terms.len() >= 2 { candidates.push(terms.join(" ")); }
    let mut individual = terms.into_iter().filter(|term| term.chars().count() >= 4).collect::<Vec<_>>();
    individual.sort_by_key(|term| std::cmp::Reverse(term.chars().count()));
    for term in individual {
        if !candidates.iter().any(|candidate| candidate.eq_ignore_ascii_case(term)) {
            candidates.push(term.to_owned());
        }
    }
    candidates
}

#[cfg(test)]
mod search_query_tests {
    use super::fallback_search_queries;

    #[test]
    fn removes_spanish_stop_words_and_adds_single_term_fallbacks() {
        assert_eq!(fallback_search_queries("La trampa de la diversidad"),
            vec!["trampa diversidad", "diversidad", "trampa"]);
    }

    #[test]
    fn does_not_create_a_fallback_for_stop_words_only() {
        assert!(fallback_search_queries("la de y").is_empty());
    }

    #[test]
    fn keeps_short_title_terms_out_of_broad_fallbacks() {
        assert_eq!(fallback_search_queries("El sol"), Vec::<String>::new());
    }
}
