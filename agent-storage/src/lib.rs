//! Persistence contracts and storage backends for agent execution data.
//!
//! This crate will define the storage abstractions used to persist runs,
//! conversation messages, and execution events. It will also provide concrete
//! storage adapters, beginning with SQLite.
//!
//! The crate will not own run scheduling or execution. Those responsibilities
//! will remain in `agent-service` and `agent-runtime`, respectively.
