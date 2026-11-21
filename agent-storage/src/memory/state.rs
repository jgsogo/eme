//! Internal maps backing the in-memory storage implementation.

use crate::{StoredMessage, StoredRun, StoredRunEvent};
use agent_metering::{ModelPricing, UsageRecord};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Default)]
pub(super) struct State {
    pub(super) runs: HashMap<Uuid, StoredRun>,
    pub(super) events: HashMap<Uuid, Vec<StoredRunEvent>>,
    pub(super) messages: HashMap<Uuid, Vec<StoredMessage>>,
    pub(super) usage: HashMap<Uuid, UsageRecord>,
    pub(super) pricing: HashMap<(String, String, String), ModelPricing>,
}
