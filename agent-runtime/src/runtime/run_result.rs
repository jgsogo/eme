use uuid::Uuid;
use crate::Message;
use super::{RunEvent, RunStatus};

pub struct RunResult {
    pub run_id: Uuid,
    pub status: RunStatus,
    pub response: Option<String>,
    pub messages: Vec<Message>,
    pub iterations: usize,
    pub events: Vec<RunEvent>,
}

