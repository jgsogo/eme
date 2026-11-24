//! Raw HTTP response data captured by the catalogue probe.

/// An uninterpreted OPAC search response, useful for inspecting live HTML.
pub struct SearchResponse {
    /// HTTP status code returned by the OPAC.
    pub status: u16,
    /// Final URL after redirects.
    pub final_url: String,
    /// Response content type, when supplied by the server.
    pub content_type: Option<String>,
    /// Raw response body.
    pub html: String,
}
