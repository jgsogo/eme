use uuid::Uuid;
use super::RunEventKind;

#[derive(Debug, Clone)]
pub struct RunEvent {
    pub run_id: Uuid,
    pub iteration: usize,
    pub kind: RunEventKind,
}

