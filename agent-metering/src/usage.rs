//! Normalized, provider-independent usage records and attribution.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// A resource quantity together with the provenance of that quantity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", content = "value", rename_all = "snake_case")]
pub enum MeteredValue {
    /// Explicitly reported by the provider.
    Reported(u64),
    /// Calculated by the platform or estimated from available information.
    Estimated(u64),
    /// The provider did not report this dimension, or it could not be derived.
    Unavailable,
}

impl MeteredValue {
    pub fn value(self) -> Option<u64> {
        match self {
            Self::Reported(value) | Self::Estimated(value) => Some(value),
            Self::Unavailable => None,
        }
    }

    pub fn is_estimated(self) -> bool {
        matches!(self, Self::Estimated(_))
    }
}

/// Named resource dimensions for one provider invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageDimensions {
    dimensions: BTreeMap<String, MeteredValue>,
}

impl Default for UsageDimensions {
    /// Standard dimensions start as unavailable, never as implicit zeroes.
    fn default() -> Self {
        Self {
            dimensions: BTreeMap::from([
                (Self::INPUT_TOKENS.into(), MeteredValue::Unavailable),
                (Self::OUTPUT_TOKENS.into(), MeteredValue::Unavailable),
                (Self::CACHED_INPUT_TOKENS.into(), MeteredValue::Unavailable),
                (Self::REASONING_TOKENS.into(), MeteredValue::Unavailable),
            ]),
        }
    }
}

impl UsageDimensions {
    pub const INPUT_TOKENS: &'static str = "input_tokens";
    pub const OUTPUT_TOKENS: &'static str = "output_tokens";
    pub const CACHED_INPUT_TOKENS: &'static str = "cached_input_tokens";
    pub const REASONING_TOKENS: &'static str = "reasoning_tokens";

    pub fn new() -> Self { Self::default() }

    pub fn with(mut self, name: impl Into<String>, value: MeteredValue) -> Self {
        self.dimensions.insert(name.into(), value);
        self
    }

    pub fn get(&self, name: &str) -> MeteredValue {
        self.dimensions.get(name).copied().unwrap_or(MeteredValue::Unavailable)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, MeteredValue)> {
        self.dimensions.iter().map(|(name, value)| (name.as_str(), *value))
    }

    pub fn is_empty(&self) -> bool {
        self.dimensions.values().all(|value| *value == MeteredValue::Unavailable)
    }
}

/// Provider-reported monetary cost, retained separately from calculated cost.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportedCost {
    /// Currency in integer micro-units (1 USD = 1,000,000 micros).
    pub amount_micros: u64,
    pub currency: String,
}

impl ReportedCost {
    pub fn new(amount_micros: u64, currency: impl Into<String>) -> Result<Self, UsageRecordError> {
        let currency = currency.into();
        if currency.len() != 3 || !currency.bytes().all(|byte| byte.is_ascii_uppercase()) {
            return Err(UsageRecordError::InvalidCurrency);
        }
        Ok(Self { amount_micros, currency })
    }
}

/// Whether the provider invocation completed from the platform's perspective.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvocationOutcome { Succeeded, Failed, TimedOut, Cancelled }

/// Immutable accounting input for one model invocation.
///
/// Every retry/fallback attempt has its own invocation ID. This makes usage
/// aggregation additive and prevents a successful retry from hiding a billable
/// failed attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageRecord {
    pub invocation_id: Uuid,
    pub run_id: Uuid,
    pub iteration: u32,
    pub agent: String,
    pub principal_id: Option<String>,
    pub provider: String,
    pub model: String,
    pub outcome: InvocationOutcome,
    pub usage: UsageDimensions,
    #[serde(default)]
    pub reported_cost: Option<ReportedCost>,
    pub occurred_at: DateTime<Utc>,
}

impl UsageRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        invocation_id: Uuid, run_id: Uuid, iteration: u32, agent: impl Into<String>,
        principal_id: Option<String>, provider: impl Into<String>, model: impl Into<String>,
        outcome: InvocationOutcome, usage: UsageDimensions, occurred_at: DateTime<Utc>,
    ) -> Result<Self, UsageRecordError> {
        let agent = agent.into();
        let provider = provider.into();
        let model = model.into();
        if agent.trim().is_empty() { return Err(UsageRecordError::EmptyAttribution("agent")); }
        if provider.trim().is_empty() { return Err(UsageRecordError::EmptyAttribution("provider")); }
        if model.trim().is_empty() { return Err(UsageRecordError::EmptyAttribution("model")); }
        Ok(Self {
            invocation_id, run_id, iteration, agent, principal_id, provider, model,
            outcome, usage, reported_cost: None, occurred_at,
        })
    }

    pub fn with_reported_cost(mut self, cost: ReportedCost) -> Self {
        self.reported_cost = Some(cost);
        self
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum UsageRecordError {
    #[error("{0} attribution must not be empty")]
    EmptyAttribution(&'static str),
    #[error("currency must be a three-letter uppercase code")]
    InvalidCurrency,
}
