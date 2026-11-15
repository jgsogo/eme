use super::Tokens;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct ExchangeInner {
    pub(super) tokens: Tokens,
}
