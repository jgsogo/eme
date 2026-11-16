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

mod family;
mod manager;
mod run;
mod runtime_executor;
mod scheduler;
mod task;

pub use family::{FamilyService, FamilyServiceError};
pub use manager::{
    CancelRunError, CancelRunOutcome, ExecutionOutcome, ExecutionReport, RunExecutor, RunManager,
    RunManagerConfigError, SubmitRunError,
};
pub use run::{InvalidRunTransition, Run, RunEvent, RunEventKind, RunId, RunStatus};
pub use runtime_executor::{AgentRuntimeExecutor, AgentRuntimeResolver, ResolvedAgent};
pub use scheduler::TaskScheduler;
pub use task::{LEGACY_TASK_OWNER, NewTask, Task, TaskError, TaskRegistry};
