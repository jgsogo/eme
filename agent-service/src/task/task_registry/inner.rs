//! Shared synchronization and storage state owned by the task registry.

use std::{collections::HashMap, path::PathBuf, sync::{Arc, RwLock}};
use tokio::sync::watch;
use uuid::Uuid;

use super::Task;

#[derive(Clone)]
pub struct TaskRegistry {
    inner: Arc<Inner>,
}
pub(super) struct Inner {
    pub(super) directory: PathBuf,
    pub(super) agents: RwLock<Vec<String>>,
    pub(super) tasks: RwLock<HashMap<Uuid, Task>>,
    pub(super) changed: watch::Sender<u64>,
}
