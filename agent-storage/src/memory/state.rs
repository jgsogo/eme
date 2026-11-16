//! Internal maps backing the in-memory storage implementation.

use crate::{StoredMessage, StoredRun, StoredRunEvent};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Default)]
pub(super) struct State {
    pub(super) runs: HashMap<Uuid, StoredRun>,
    pub(super) events: HashMap<Uuid, Vec<StoredRunEvent>>,
    pub(super) messages: HashMap<Uuid, Vec<StoredMessage>>,
}
