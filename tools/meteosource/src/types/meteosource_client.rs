use reqwest::Client;

/// Small client for the Meteosource place-search and point-weather endpoints.
pub(crate) struct MeteosourceClient {
    pub(crate) client: Client,
    pub(crate) api_key: String,
    pub(crate) base_url: String,
}
