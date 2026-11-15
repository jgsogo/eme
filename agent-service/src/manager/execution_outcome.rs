/// Outcome reported by a run executor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionOutcome {
    Completed(Option<String>),
    Cancelled,
    TimedOut,
    MaxIterationsReached,
    Failed(String),
}
