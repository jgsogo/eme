//! Internal reasons that cause the runtime execution loop to stop.

#[derive(Debug, Clone, Copy)]
pub(super) enum StopReason {
    Cancelled,
    TimedOut,
}
