use super::super::*;

#[derive(Debug, Deserialize)]
pub(super) struct ExchangeInner {
    pub(super) tokens: Tokens,
}
