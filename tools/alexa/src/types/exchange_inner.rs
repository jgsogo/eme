use super::*;

#[derive(Debug, Deserialize)]
struct ExchangeInner {
    pub(super) tokens: Tokens,
}
