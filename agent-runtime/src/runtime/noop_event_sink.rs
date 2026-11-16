//! Event sink implementation that intentionally discards runtime events.

use super::{EventSink, RunEvent};

pub struct NoopEventSink;

impl EventSink for NoopEventSink {
    fn emit(&self, _event: &RunEvent) {}
}
