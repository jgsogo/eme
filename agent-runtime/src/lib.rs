//! Execution engine for an individual AI agent run.
//!
//! This crate will coordinate the agent conversation loop, model adaptation,
//! tool registration and dispatch, argument validation, deterministic
//! authorization, execution limits, cancellation, and runtime events.
//!
//! It will execute individual runs. Managing concurrent runs and their
//! service-level lifecycle will belong to `agent-service`; provider-specific
//! integrations will belong to `agent-models`; persistence and external
//! transport will belong to their respective platform crates.
