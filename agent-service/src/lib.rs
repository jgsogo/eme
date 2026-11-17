//! Orchestration and lifecycle management for concurrent agent runs.
//!
//! This crate owns run submission, worker coordination, cancellation,
//! supervision, and event routing. Task definitions and scheduling live in
//! the dedicated `tasks-tool` crate.

mod family;
mod manager;
mod run;
mod runtime_executor;

pub use family::{FamilyService, FamilyServiceError};
pub use manager::{
    CancelRunError, CancelRunOutcome, ExecutionOutcome, ExecutionReport, RunExecutor, RunManager,
    RunManagerConfigError, SubmitRunError,
};
pub use run::{InvalidRunTransition, Run, RunEvent, RunEventKind, RunId, RunStatus};
pub use runtime_executor::{AgentRuntimeExecutor, AgentRuntimeResolver, ResolvedAgent};
