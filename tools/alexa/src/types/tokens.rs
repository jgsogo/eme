use super::*;

#[derive(Debug, Deserialize)]
pub(super) struct Tokens {
    pub(super) cookies: HashMap<String, Vec<CookieEntry>>,
}
