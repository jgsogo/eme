//! Internal maps backing the in-memory storage implementation.

use std::collections::HashMap;
use uuid::Uuid;
use crate::{StoredMessage, StoredRun, StoredRunEvent};

#[derive(Default)]
pub(super) struct State {
    pub(super) runs: HashMap<Uuid, StoredRun>,
    pub(super) events: HashMap<Uuid, Vec<StoredRunEvent>>,
    pub(super) messages: HashMap<Uuid, Vec<StoredMessage>>,
}
