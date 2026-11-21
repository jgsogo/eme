//! Versioned model pricing and deterministic integer cost calculation.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{MeteredValue, ReportedCost, UsageRecord};

const UNITS_PER_MILLION: u128 = 1_000_000;

/// Rates are currency micro-units per one million resource units.
///
/// A missing rate means that the dimension is not priced, not that it is free.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelPricing {
    pub provider: String,
    pub model: String,
    pub version: String,
    pub currency: String,
    pub effective_from: DateTime<Utc>,
    pub effective_until: Option<DateTime<Utc>>,
    pub rates_per_million: BTreeMap<String, Option<u64>>,
}

impl ModelPricing {
    pub fn applies_to(&self, usage: &UsageRecord) -> bool {
        self.provider == usage.provider
            && self.model == usage.model
            && usage.occurred_at >= self.effective_from
            && self
                .effective_until
                .is_none_or(|until| usage.occurred_at < until)
    }

    pub fn validate(&self) -> Result<(), PricingError> {
        if self.provider.trim().is_empty() || self.model.trim().is_empty() {
            return Err(PricingError::EmptyModelIdentity);
        }
        if self.version.trim().is_empty() {
            return Err(PricingError::EmptyVersion);
        }
        if self.currency.len() != 3 || !self.currency.bytes().all(|byte| byte.is_ascii_uppercase())
        {
            return Err(PricingError::InvalidCurrency);
        }
        if self
            .effective_until
            .is_some_and(|until| until <= self.effective_from)
        {
            return Err(PricingError::InvalidEffectiveRange);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostStatus {
    Complete,
    Partial,
    Unavailable,
}

/// Explains where a monetary amount came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostOrigin {
    ProviderReported,
    Calculated,
    Estimated,
}

/// A reproducible cost calculation in integer currency micro-units.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CostEstimate {
    pub amount_micros: u64,
    pub currency: String,
    pub pricing_version: String,
    pub status: CostStatus,
    pub origin: CostOrigin,
    pub contains_estimated_usage: bool,
}

impl CostEstimate {
    /// Converts provider-reported cost without replacing or mutating the raw record.
    pub fn from_reported(cost: &ReportedCost) -> Self {
        Self {
            amount_micros: cost.amount_micros,
            currency: cost.currency.clone(),
            pricing_version: "provider".into(),
            status: CostStatus::Complete,
            origin: CostOrigin::ProviderReported,
            contains_estimated_usage: false,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PricingError {
    #[error("pricing provider and model must not be empty")]
    EmptyModelIdentity,
    #[error("pricing version must not be empty")]
    EmptyVersion,
    #[error("currency must be a three-letter uppercase code")]
    InvalidCurrency,
    #[error("pricing effective_until must be later than effective_from")]
    InvalidEffectiveRange,
    #[error("cost calculation overflowed the supported integer range")]
    Overflow,
    #[error("pricing does not apply to this usage record")]
    PricingDoesNotApply,
}

/// Calculates cost using one immutable pricing version. Unknown or unpriced
/// dimensions make the result partial; estimated quantities remain marked.
pub fn calculate_cost(
    usage: &UsageRecord,
    pricing: &ModelPricing,
) -> Result<CostEstimate, PricingError> {
    pricing.validate()?;
    if !pricing.applies_to(usage) {
        return Err(PricingError::PricingDoesNotApply);
    }

    let mut amount = 0_u128;
    let mut complete = true;
    let mut contains_estimated_usage = false;
    let mut priced_dimensions = 0_usize;

    for (name, quantity) in usage.usage.iter() {
        let Some(rate) = pricing.rates_per_million.get(name).copied().flatten() else {
            complete = false;
            continue;
        };
        let Some(value) = quantity.value() else {
            complete = false;
            continue;
        };
        contains_estimated_usage |= quantity.is_estimated();
        let dimension_cost = u128::from(value)
            .checked_mul(u128::from(rate))
            .ok_or(PricingError::Overflow)?
            / UNITS_PER_MILLION;
        amount = amount
            .checked_add(dimension_cost)
            .ok_or(PricingError::Overflow)?;
        priced_dimensions += 1;
    }

    let status = if priced_dimensions == 0 {
        CostStatus::Unavailable
    } else if complete {
        CostStatus::Complete
    } else {
        CostStatus::Partial
    };

    Ok(CostEstimate {
        amount_micros: u64::try_from(amount).map_err(|_| PricingError::Overflow)?,
        currency: pricing.currency.clone(),
        pricing_version: pricing.version.clone(),
        status,
        origin: if contains_estimated_usage {
            CostOrigin::Estimated
        } else {
            CostOrigin::Calculated
        },
        contains_estimated_usage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InvocationOutcome, UsageDimensions};
    use uuid::Uuid;

    fn usage(dimensions: UsageDimensions) -> UsageRecord {
        UsageRecord::new(
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            1,
            "assistant",
            Some("user-1".into()),
            "openai",
            "gpt-test",
            InvocationOutcome::Succeeded,
            dimensions,
            DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        )
        .unwrap()
    }

    fn pricing() -> ModelPricing {
        ModelPricing {
            provider: "openai".into(),
            model: "gpt-test".into(),
            version: "2026-01".into(),
            currency: "USD".into(),
            effective_from: DateTime::parse_from_rfc3339("2025-12-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            effective_until: None,
            rates_per_million: BTreeMap::from([
                (UsageDimensions::INPUT_TOKENS.into(), Some(2_000_000)),
                (UsageDimensions::OUTPUT_TOKENS.into(), Some(8_000_000)),
                (UsageDimensions::CACHED_INPUT_TOKENS.into(), Some(500_000)),
                (UsageDimensions::REASONING_TOKENS.into(), Some(8_000_000)),
            ]),
        }
    }

    #[test]
    fn calculates_cost_with_integer_arithmetic() {
        let record = usage(
            UsageDimensions::new()
                .with(UsageDimensions::INPUT_TOKENS, MeteredValue::Reported(1_000))
                .with(UsageDimensions::OUTPUT_TOKENS, MeteredValue::Reported(500)),
        );
        let cost = calculate_cost(&record, &pricing()).unwrap();
        assert_eq!(cost.amount_micros, 6_000);
        assert_eq!(cost.status, CostStatus::Partial);
        assert_eq!(cost.origin, CostOrigin::Calculated);
    }

    #[test]
    fn preserves_estimated_and_unavailable_usage_semantics() {
        let record = usage(
            UsageDimensions::new()
                .with(
                    UsageDimensions::INPUT_TOKENS,
                    MeteredValue::Estimated(1_000),
                )
                .with(UsageDimensions::OUTPUT_TOKENS, MeteredValue::Reported(0))
                .with(
                    UsageDimensions::CACHED_INPUT_TOKENS,
                    MeteredValue::Reported(0),
                )
                .with(UsageDimensions::REASONING_TOKENS, MeteredValue::Reported(0)),
        );
        let cost = calculate_cost(&record, &pricing()).unwrap();
        assert_eq!(cost.amount_micros, 2_000);
        assert_eq!(cost.status, CostStatus::Complete);
        assert_eq!(cost.origin, CostOrigin::Estimated);
        assert!(cost.contains_estimated_usage);
    }

    #[test]
    fn refuses_to_apply_a_different_model_price() {
        let record = usage(UsageDimensions::new());
        let mut price = pricing();
        price.model = "other-model".into();
        assert_eq!(
            calculate_cost(&record, &price),
            Err(PricingError::PricingDoesNotApply)
        );
    }

    #[test]
    fn missing_dimensions_are_not_silently_treated_as_zero() {
        let record = usage(
            UsageDimensions::new().with(UsageDimensions::INPUT_TOKENS, MeteredValue::Reported(10)),
        );
        let cost = calculate_cost(&record, &pricing()).unwrap();
        assert_eq!(cost.amount_micros, 20);
        assert_eq!(cost.status, CostStatus::Partial);
    }

    #[test]
    fn provider_reported_cost_is_distinguished_from_calculated_cost() {
        let reported = ReportedCost::new(12_345, "USD").unwrap();
        let cost = CostEstimate::from_reported(&reported);
        assert_eq!(cost.origin, CostOrigin::ProviderReported);
        assert_eq!(cost.amount_micros, 12_345);
    }
}
