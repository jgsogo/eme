//! Aggregation of invocation-level usage into run-level accounting summaries.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{CostEstimate, CostOrigin, CostStatus, ModelPricing, UsageRecord, calculate_cost};

/// Totals for one normalized resource dimension.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DimensionUsageSummary {
    pub reported: u64,
    pub estimated: u64,
    pub unavailable_invocations: u64,
}

/// Cost totals retain the source of every amount.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrencyCostSummary {
    pub total_micros: u64,
    pub provider_reported_micros: u64,
    pub calculated_micros: u64,
    pub estimated_micros: u64,
    pub partial_invocations: u64,
    pub unavailable_invocations: u64,
}

/// Run-level totals across all model invocations, retries, and fallbacks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageSummary {
    pub invocation_count: u64,
    pub succeeded_count: u64,
    pub failed_count: u64,
    pub timed_out_count: u64,
    pub cancelled_count: u64,
    pub incomplete_invocations: u64,
    pub unpriced_invocations: u64,
    pub dimensions: BTreeMap<String, DimensionUsageSummary>,
    pub costs: BTreeMap<String, CurrencyCostSummary>,
}

/// Aggregates records additively. Each invocation ID is expected to be unique;
/// retries and fallback attempts are separate records and therefore counted.
pub fn summarize_usage(records: &[UsageRecord], pricing: &[ModelPricing]) -> UsageSummary {
    let mut summary = UsageSummary::default();

    for record in records {
        summary.invocation_count = summary.invocation_count.saturating_add(1);
        match record.outcome {
            crate::InvocationOutcome::Succeeded => summary.succeeded_count += 1,
            crate::InvocationOutcome::Failed => summary.failed_count += 1,
            crate::InvocationOutcome::TimedOut => summary.timed_out_count += 1,
            crate::InvocationOutcome::Cancelled => summary.cancelled_count += 1,
        }

        let mut incomplete = false;
        for (name, value) in record.usage.iter() {
            let total = summary.dimensions.entry(name.to_owned()).or_default();
            match value {
                crate::MeteredValue::Reported(value) => {
                    total.reported = total.reported.saturating_add(value);
                }
                crate::MeteredValue::Estimated(value) => {
                    total.estimated = total.estimated.saturating_add(value);
                    incomplete = true;
                }
                crate::MeteredValue::Unavailable => {
                    total.unavailable_invocations = total.unavailable_invocations.saturating_add(1);
                    incomplete = true;
                }
            }
        }

        let cost = record
            .reported_cost
            .as_ref()
            .map(CostEstimate::from_reported)
            .or_else(|| {
                pricing
                    .iter()
                    .find(|price| price.applies_to(record))
                    .and_then(|price| calculate_cost(record, price).ok())
            });

        match cost {
            Some(cost) if cost.status != CostStatus::Unavailable => {
                let totals = summary.costs.entry(cost.currency.clone()).or_default();
                totals.total_micros = totals.total_micros.saturating_add(cost.amount_micros);
                match cost.origin {
                    CostOrigin::ProviderReported => {
                        totals.provider_reported_micros = totals
                            .provider_reported_micros
                            .saturating_add(cost.amount_micros)
                    }
                    CostOrigin::Calculated => {
                        totals.calculated_micros =
                            totals.calculated_micros.saturating_add(cost.amount_micros)
                    }
                    CostOrigin::Estimated => {
                        totals.estimated_micros =
                            totals.estimated_micros.saturating_add(cost.amount_micros)
                    }
                }
                if cost.status == CostStatus::Partial {
                    totals.partial_invocations = totals.partial_invocations.saturating_add(1);
                    incomplete = true;
                }
            }
            Some(cost) => {
                let totals = summary.costs.entry(cost.currency).or_default();
                totals.unavailable_invocations = totals.unavailable_invocations.saturating_add(1);
                summary.unpriced_invocations = summary.unpriced_invocations.saturating_add(1);
                incomplete = true;
            }
            None => {
                summary.unpriced_invocations = summary.unpriced_invocations.saturating_add(1);
                incomplete = true;
            }
        }

        if incomplete {
            summary.incomplete_invocations = summary.incomplete_invocations.saturating_add(1);
        }
    }

    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InvocationOutcome, MeteredValue, ReportedCost, UsageDimensions};
    use chrono::{DateTime, Utc};
    use std::collections::BTreeMap;
    use uuid::Uuid;

    fn record(
        id: u128,
        outcome: InvocationOutcome,
        usage: UsageDimensions,
        reported_cost: Option<ReportedCost>,
    ) -> UsageRecord {
        let record = UsageRecord::new(
            Uuid::from_u128(id),
            Uuid::from_u128(99),
            1,
            "assistant",
            Some("user-1".into()),
            "openai",
            "gpt-test",
            outcome,
            usage,
            DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        )
        .unwrap();
        reported_cost.map_or(record.clone(), |cost| record.with_reported_cost(cost))
    }

    fn pricing() -> ModelPricing {
        ModelPricing {
            provider: "openai".into(),
            model: "gpt-test".into(),
            version: "v1".into(),
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
    fn aggregates_failed_and_successful_fallback_attempts_without_dropping_either() {
        let records = vec![
            record(
                1,
                InvocationOutcome::Failed,
                UsageDimensions::new()
                    .with(UsageDimensions::INPUT_TOKENS, MeteredValue::Estimated(100)),
                None,
            ),
            record(
                2,
                InvocationOutcome::Succeeded,
                UsageDimensions::new()
                    .with(UsageDimensions::INPUT_TOKENS, MeteredValue::Reported(1_000))
                    .with(UsageDimensions::OUTPUT_TOKENS, MeteredValue::Reported(500))
                    .with(
                        UsageDimensions::CACHED_INPUT_TOKENS,
                        MeteredValue::Reported(0),
                    )
                    .with(UsageDimensions::REASONING_TOKENS, MeteredValue::Reported(0)),
                None,
            ),
        ];

        let summary = summarize_usage(&records, &[pricing()]);

        assert_eq!(summary.invocation_count, 2);
        assert_eq!(summary.failed_count, 1);
        assert_eq!(summary.succeeded_count, 1);
        assert_eq!(
            summary.dimensions[UsageDimensions::INPUT_TOKENS].reported,
            1_000
        );
        assert_eq!(
            summary.dimensions[UsageDimensions::INPUT_TOKENS].estimated,
            100
        );
        assert_eq!(summary.costs["USD"].calculated_micros, 6_000);
        assert_eq!(summary.costs["USD"].estimated_micros, 200);
        assert_eq!(summary.costs["USD"].partial_invocations, 1);
        assert_eq!(summary.unpriced_invocations, 0);
        assert_eq!(summary.incomplete_invocations, 1);
    }

    #[test]
    fn provider_reported_cost_takes_precedence_over_catalogue_calculation() {
        let record = record(
            3,
            InvocationOutcome::Succeeded,
            UsageDimensions::new(),
            Some(ReportedCost::new(123, "USD").unwrap()),
        );

        let summary = summarize_usage(&[record], &[pricing()]);

        assert_eq!(summary.costs["USD"].provider_reported_micros, 123);
        assert_eq!(summary.costs["USD"].calculated_micros, 0);
    }
}
