//! Orchestration and lifecycle management for concurrent agent runs.
//!
//! This crate owns service-level run submission, scheduling, worker
//! coordination, cancellation, supervision, and run-event routing. It remains
//! independent of HTTP transport and concrete storage backends.

mod manager;
mod run;
mod runtime_executor;

pub use manager::{
    ExecutionOutcome, RunExecutor, RunManager, RunManagerConfigError, SubmitRunError,
};
pub use run::{InvalidRunTransition, Run, RunId, RunStatus};
pub use runtime_executor::{AgentRuntimeExecutor, AgentRuntimeResolver, ResolvedAgent};
