use super::*;

pub(crate) struct TavilyClient {
    client: Client,
    api_key: String,
    base_url: String,
}
