use super::*;

#[derive(Debug, Deserialize)]
pub(super) struct ExchangeInner {
    pub(super) tokens: Tokens,
}
