use super::CookieEntry;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub(crate) struct Tokens {
    pub(crate) cookies: HashMap<String, Vec<CookieEntry>>,
}
