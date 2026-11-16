//! Event envelope identifying the run and iteration associated with a lifecycle event.

use super::RunEventKind;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct RunEvent {
    pub run_id: Uuid,
    pub iteration: usize,
    pub kind: RunEventKind,
}
