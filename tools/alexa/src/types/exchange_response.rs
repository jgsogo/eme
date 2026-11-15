use super::super::*;

#[derive(Debug, Deserialize)]
pub(super) struct ExchangeResponse {
    pub(super) response: ExchangeInner,
}
