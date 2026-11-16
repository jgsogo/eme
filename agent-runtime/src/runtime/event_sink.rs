//! Event sink contract used to observe agent runtime lifecycle events.

use super::RunEvent;

/// Receives runtime events as they occur.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: &RunEvent);
}
