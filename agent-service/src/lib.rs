//! Orchestration and lifecycle management for concurrent agent runs.
//!
//! This crate owns service-level run submission, scheduling, worker coordination,
//! cancellation, supervision, and event routing. It provides application-facing
//! services that combine the agent runtime with storage-backed task definitions.
//!
//! The crate deliberately stays independent of HTTP, messaging, and other transport
//! protocols. Transport adapters authenticate requests and provide trusted actor
//! context; storage adapters implement persistence behind the interfaces consumed
//! by the service layer.
