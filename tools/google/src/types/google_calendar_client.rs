use reqwest::Client;

pub(crate) struct GoogleCalendarClient {
    pub(crate) client: Client,
    pub(crate) access_token: String,
    pub(crate) base_url: String,
}
