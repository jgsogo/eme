use super::ExchangeInner;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct ExchangeResponse {
    pub(super) response: ExchangeInner,
}
