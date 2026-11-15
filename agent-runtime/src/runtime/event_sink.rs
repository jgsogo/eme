use super::RunEvent;
use std::sync::Mutex;

/// Receives runtime events as they occur.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: &RunEvent);
}

pub struct NoopEventSink;

impl EventSink for NoopEventSink {
    fn emit(&self, _event: &RunEvent) {}
}

/// An event sink that stores events in memory.
#[derive(Default)]
pub struct CollectingEventSink {
    events: Mutex<Vec<RunEvent>>,
}

impl CollectingEventSink {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a snapshot of all collected events.
    pub fn events(&self) -> Vec<RunEvent> {
        self.events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

impl EventSink for CollectingEventSink {
    fn emit(&self, event: &RunEvent) {
        self.events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(event.clone());
    }
}
