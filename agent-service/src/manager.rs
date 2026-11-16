//! Run admission, execution coordination, and lifecycle supervision.

mod cancel_run_error;
mod cancel_run_outcome;
mod execution_outcome;
mod execution_report;
mod managed_run;
mod run_executor;
mod run_manager;
mod run_manager_config_error;
mod submit_run_error;
#[cfg(test)]
mod tests;

pub use cancel_run_error::CancelRunError;
pub use cancel_run_outcome::CancelRunOutcome;
pub use execution_outcome::ExecutionOutcome;
pub use execution_report::ExecutionReport;
use managed_run::ManagedRun;
pub use run_executor::RunExecutor;
pub use run_manager::RunManager;
pub use run_manager_config_error::RunManagerConfigError;
pub use submit_run_error::SubmitRunError;
