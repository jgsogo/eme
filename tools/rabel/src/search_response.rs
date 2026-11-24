//! Raw HTTP response data captured by the catalogue probe.

/// An OPAC response, with optional session URL for follow-up record requests.
pub struct SearchResponse {
    /// HTTP status code returned by the OPAC.
    pub status: u16,
    /// Final URL after HTTP redirects.
    pub final_url: String,
    /// Response content type, when supplied by the server.
    pub content_type: Option<String>,
    /// Session-specific base URL extracted from the OPAC meta refresh.
    pub session_url: Option<String>,
    /// Raw response body.
    pub html: String,
}
