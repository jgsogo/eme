use reqwest::Client;

pub(crate) struct TavilyClient {
    pub(crate) client: Client,
    pub(crate) api_key: String,
    pub(crate) base_url: String,
}
