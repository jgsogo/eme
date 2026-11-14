//! Orchestration and lifecycle management for concurrent agent runs.
//!
//! This crate owns service-level run submission, scheduling, worker
//! coordination, cancellation, supervision, and run-event routing. It remains
//! independent of HTTP transport and concrete storage backends.

mod run;

pub use run::{InvalidRunTransition, Run, RunId, RunStatus};
