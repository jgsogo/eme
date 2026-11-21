/// Provider-reported monetary cost from a model response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderReportedCost {
    /// Cost in integer currency micro-units.
    pub amount_micros: u64,
    pub currency: String,
}

impl ProviderReportedCost {
    pub fn new(amount_micros: u64, currency: impl Into<String>) -> Option<Self> {
        let currency = currency.into();
        if currency.len() != 3 || !currency.bytes().all(|byte| byte.is_ascii_uppercase()) {
            return None;
        }
        Some(Self {
            amount_micros,
            currency,
        })
    }
}
