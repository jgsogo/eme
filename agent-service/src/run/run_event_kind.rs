//! Kinds of events emitted during a run lifecycle.

/// An event recorded in the ordered history of a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEventKind {
    Queued,
    Started,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
    MaxIterationsReached,
    ModelRequested,
    ModelResponded,
    ToolExecutionStarted,
    ToolExecutionFinished,
    ModelFailed,
    RuntimeCancelled,
    RuntimeTimedOut,
}
