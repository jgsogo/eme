use reqwest::Client;

pub(crate) struct TavilyClient {
    pub(super) client: Client,
    pub(super) api_key: String,
    pub(super) base_url: String,
}
