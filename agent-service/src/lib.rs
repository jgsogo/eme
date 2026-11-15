//! Orchestration and lifecycle management for concurrent agent runs.
//!
//! This crate owns service-level run submission, scheduling, worker
//! coordination, cancellation, supervision, and run-event routing. It remains
//! independent of HTTP transport and concrete storage backends.

mod manager;
mod run;
mod runtime_executor;
mod scheduler;
mod task;

pub use manager::{
    CancelRunError, CancelRunOutcome, ExecutionOutcome, ExecutionReport, RunExecutor, RunManager,
    RunManagerConfigError, SubmitRunError,
};
pub use run::{InvalidRunTransition, Run, RunEvent, RunEventKind, RunId, RunStatus};
pub use runtime_executor::{AgentRuntimeExecutor, AgentRuntimeResolver, ResolvedAgent};
pub use scheduler::TaskScheduler;
pub use task::TaskRegistry;
