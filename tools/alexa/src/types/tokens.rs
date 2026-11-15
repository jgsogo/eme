use super::CookieEntry;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub(super) struct Tokens {
    pub(super) cookies: HashMap<String, Vec<CookieEntry>>,
}
