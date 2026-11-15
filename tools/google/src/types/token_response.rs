use super::*;

#[derive(Debug, Deserialize)]
struct TokenResponse {
    pub(super) access_token: String,
}
