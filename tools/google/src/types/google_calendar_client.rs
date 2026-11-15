use super::*;

struct GoogleCalendarClient {
    pub(super) client: Client,
    pub(super) access_token: String,
    pub(super) base_url: String,
}
