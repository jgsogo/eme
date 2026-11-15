pub(super) struct GeminiClient {
    pub(super) http: Client,
    pub(super) base_url: String,
    pub(super) api_key: String,
    pub(super) model: String,
}
