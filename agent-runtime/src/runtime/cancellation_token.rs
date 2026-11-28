//! Shared cancellation signal and its operation-boundary semantics.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Shared cancellation signal for an agent run.
///
/// Cancellation is observed at operation boundaries. A model request or tool
/// invocation that has already started is allowed to finish; its result is
/// recorded before the runtime stops. No subsequent model or tool operation is
/// started after cancellation is observed. This avoids abandoning a tool result
/// when an external side effect may already have occurred.
#[derive(Clone, Default, Debug)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation of the run.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    /// Returns whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

impl PartialEq for CancellationToken {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.cancelled, &other.cancelled)
    }
}

impl Eq for CancellationToken {}
