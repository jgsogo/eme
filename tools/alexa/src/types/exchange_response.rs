use super::*;

#[derive(Debug, Deserialize)]
struct ExchangeResponse {
    pub(super) response: ExchangeInner,
}
