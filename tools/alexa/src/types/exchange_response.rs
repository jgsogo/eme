use super::ExchangeInner;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct ExchangeResponse {
    pub(crate) response: ExchangeInner,
}
