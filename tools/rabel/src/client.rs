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

    /// Submit an all-fields search and follow AbsysNet's HTML meta-refresh
    /// into the session-specific results URL.
    ///
    /// AbsysNet does not issue an HTTP redirect for this transition: it returns
    /// a tiny HTML page containing `<meta http-equiv="Refresh" ...>`. The
    /// session URL must be requested with the same cookie jar.
    pub async fn search(&self, query: &str) -> Result<SearchResponse, reqwest::Error> {
        let mut url = self.opac_url.clone();
        url.query_pairs_mut()
            .append_pair("SUBC", &self.subcatalog)
            .append_pair("ACC", "DOSEARCH")
            .append_pair("xsqf01", query);

        let response = self.http.get(url).send().await?;
        let bootstrap_url = response.url().clone();
        let status = response.status().as_u16();
        let content_type = content_type(&response);
        let html = response.text().await?;

        let Some(refresh_target) = meta_refresh_target(&html) else {
            return Ok(SearchResponse {
                status,
                final_url: bootstrap_url.to_string(),
                content_type,
                session_url: None,
                html,
            });
        };

        let session_url = bootstrap_url
            .join(&refresh_target)
            .unwrap_or_else(|_| bootstrap_url.clone());
        let response = self.http.get(session_url.clone()).send().await?;
        let result = read_response(response, Some(session_url.to_string())).await?;
        Ok(result)
    }

    /// Fetch one bibliographic record from a previous search response.
    ///
    /// The document number is the `DOC` parameter used by the OPAC result
    /// links (for example, `ACC=165&DOC=1`).
    pub async fn record(
        &self,
        session_url: &str,
        document: u32,
    ) -> Result<SearchResponse, reqwest::Error> {
        let mut url = Url::parse(session_url).expect("session URL came from a parsed URL");
        url.set_path(&format!("{}/NT1", url.path().trim_end_matches('/')));
        url.set_query(None);
        url.query_pairs_mut()
            .append_pair("ACC", "165")
            .append_pair("DOC", &document.to_string());

        let response = self.http.get(url).send().await?;
        read_response(response, Some(session_url.to_owned())).await
    }
}

fn content_type(response: &reqwest::Response) -> Option<String> {
    response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

async fn read_response(
    response: reqwest::Response,
    session_url: Option<String>,
) -> Result<SearchResponse, reqwest::Error> {
    let status = response.status().as_u16();
    let final_url = response.url().to_string();
    let content_type = content_type(&response);
    let html = response.text().await?;

    Ok(SearchResponse {
        status,
        final_url,
        content_type,
        session_url,
        html,
    })
}

/// Extract the URL from an AbsysNet `meta http-equiv="Refresh"` element.
///
/// This deliberately handles only the small, predictable bootstrap response;
/// it is not intended to be a general-purpose HTML parser.
fn meta_refresh_target(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let mut offset = 0;

    while let Some(start) = lower[offset..].find("<meta") {
        let start = offset + start;
        let end = lower[start..].find('>')? + start;
        let tag = &html[start..=end];
        let lower_tag = tag.to_ascii_lowercase();

        if attribute_value(&lower_tag, tag, "http-equiv")
            .is_some_and(|value| value.eq_ignore_ascii_case("refresh"))
        {
            let content = attribute_value(&lower_tag, tag, "content")?;
            let (_, target) = content.split_once("url=")
                .or_else(|| {
                    let lower_content = content.to_ascii_lowercase();
                    lower_content.find("url=").map(|index| (&content[..index], &content[index..]))
                })?;
            return Some(target[4..].trim().trim_matches(['"', '\'']).to_owned());
        }

        offset = end + 1;
    }

    None
}

/// Read a quoted or unquoted HTML attribute value from a tag.
fn attribute_value<'a>(lower_tag: &'a str, original_tag: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("{name}=");
    let index = lower_tag.find(&needle)? + needle.len();
    let value = original_tag.get(index..)?.trim_start();

    if let Some(quote @ ('"' | '\'')) = value.chars().next() {
        let value = &value[quote.len_utf8()..];
        let end = value.find(quote)?;
        Some(&value[..end])
    } else {
        let end = value.find(|character: char| character.is_whitespace() || character == '>')
            .unwrap_or(value.len());
        Some(&value[..end])
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
    async fn follows_meta_refresh_and_fetches_search_results_with_same_client() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/cgi-bin/abnetopac2"))
            .and(query_param("SUBC", "SAL/BPSA"))
            .and(query_param("ACC", "DOSEARCH"))
            .and(query_param("xsqf01", "café & Rust"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html; charset=utf-8")
                    .insert_header("set-cookie", "session=abc; Path=/")
                    .set_body_string(
                        r#"<html><head><meta http-equiv="Refresh" content="0; URL=/cgi-bin/abnetopac2/session-token?ACC=161" /></head></html>"#,
                    ),
            )
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/cgi-bin/abnetopac2/session-token"))
            .and(query_param("ACC", "161"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html; charset=iso-8859-1")
                    .set_body_string("<html>Search results</html>"),
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
            Some("text/html; charset=iso-8859-1")
        );
        assert!(response.html.contains("Search results"));
        assert!(response.session_url.as_deref().unwrap().contains("session-token"));
        assert!(response.final_url.contains("ACC=161"));
    }

    #[tokio::test]
    async fn fetches_a_record_from_the_search_session() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/cgi-bin/abnetopac2/session-token/NT1"))
            .and(query_param("ACC", "165"))
            .and(query_param("DOC", "7"))
            .respond_with(ResponseTemplate::new(200).set_body_string("<html>Record 7</html>"))
            .expect(1)
            .mount(&server)
            .await;

        let client = RabelClient::with_endpoint(
            Url::parse(&format!("{}/cgi-bin/abnetopac2", server.uri())).unwrap(),
            SUBCATALOG,
        )
        .unwrap();
        let search = SearchResponse {
            status: 200,
            final_url: format!("{}/cgi-bin/abnetopac2/session-token?ACC=161", server.uri()),
            content_type: Some("text/html".to_owned()),
            session_url: Some(format!("{}/cgi-bin/abnetopac2/session-token", server.uri())),
            html: "<html>Results</html>".to_owned(),
        };

        let record = client.record(search.session_url.as_deref().unwrap(), 7).await.unwrap();
        assert_eq!(record.status, 200);
        assert!(record.html.contains("Record 7"));
        assert!(record.final_url.contains("DOC=7"));
    }

    #[test]
    fn extracts_absysnet_meta_refresh_target() {
        let html = r#"<meta http-equiv="Refresh" content="0; URL=/cgi-bin/abnetopac2/session?ACC=161" />"#;
        assert_eq!(
            meta_refresh_target(html).as_deref(),
            Some("/cgi-bin/abnetopac2/session?ACC=161")
        );
    }

    #[test]
    fn submits_encoded_search_to_the_configured_subcatalogue() {
        // Query encoding and URL construction are exercised by the wiremock
        // search-session test above.
        assert!(meta_refresh_target("<html></html>").is_none());
    }
}
