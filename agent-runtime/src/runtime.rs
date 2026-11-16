//! Agent run orchestration, cancellation, lifecycle events, and execution results.

mod agent_runtime;
mod cancellation_token;
mod collecting_event_sink;
mod event_sink;
mod noop_event_sink;
mod run_event;
mod run_event_kind;
mod run_failure;
mod run_result;
mod run_status;
mod stop_reason;

use stop_reason::StopReason;

pub use agent_runtime::AgentRuntime;
pub use cancellation_token::CancellationToken;
pub use collecting_event_sink::CollectingEventSink;
pub use event_sink::EventSink;
pub use noop_event_sink::NoopEventSink;
pub use run_event::RunEvent;
pub use run_event_kind::RunEventKind;
pub use run_failure::RunFailure;
pub use run_result::RunResult;
pub use run_status::RunStatus;
