//! Provider-independent model integration for the AI agent platform.
//!
//! This crate will define the common conversation contract and client
//! abstraction consumed by the agent runtime. It will provide provider
//! adapters, provider configuration loading, model resolution, capability
//! metadata, and a common model error vocabulary.
//!
//! Agent execution, tool authorization, and run lifecycle management will
//! remain outside this crate.

pub mod client;
pub mod error;
pub mod models;
pub mod providers;
pub mod registry;
pub mod types;
