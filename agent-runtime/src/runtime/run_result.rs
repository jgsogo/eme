use super::{RunEvent, RunStatus};
use crate::Message;
use uuid::Uuid;

pub struct RunResult {
    pub run_id: Uuid,
    pub status: RunStatus,
    pub response: Option<String>,
    pub messages: Vec<Message>,
    pub iterations: usize,
    pub events: Vec<RunEvent>,
}
