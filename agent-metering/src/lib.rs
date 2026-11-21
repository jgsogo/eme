//! Provider-independent usage accounting primitives.
//!
//! This crate defines normalized, durable-domain representations for resource
//! consumption at the model-invocation level, including provenance, attribution,
//! and provider-reported monetary amounts. It deliberately has no dependency on
//! model providers, storage engines, HTTP APIs, or monitoring backends.
//!
//! Pricing data is versioned and cost calculations use integer arithmetic.
//! Storage and run orchestration are owned by the storage and service crates;
//! this crate owns the accounting vocabulary and deterministic calculations.

mod pricing;
mod usage;

pub use pricing::{
    CostEstimate, CostOrigin, CostStatus, ModelPricing, PricingError, calculate_cost,
};
pub use usage::{
    InvocationOutcome, MeteredValue, ReportedCost, UsageDimensions, UsageRecord, UsageRecordError,
};
