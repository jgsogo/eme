use super::super::*;

/// Small client for the Meteosource place-search and point-weather endpoints.
pub(crate) struct MeteosourceClient {
    pub(super) client: Client,
    pub(super) api_key: String,
    pub(super) base_url: String,
}
