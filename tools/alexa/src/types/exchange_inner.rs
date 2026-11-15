use super::Tokens;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct ExchangeInner {
    pub(crate) tokens: Tokens,
}
