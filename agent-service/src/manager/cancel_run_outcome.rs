/// Result of requesting cancellation for a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelRunOutcome {
    /// Cancellation was signalled to a running execution.
    Requested,
    /// The queued run was cancelled before a worker started it.
    CancelledBeforeStart,
    /// A cancellation request had already been sent.
    AlreadyRequested,
    /// The run had already reached a terminal state.
    AlreadyFinished,
}
