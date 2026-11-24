//! Persistence contracts and data transfer objects for agent execution.

mod conversation_message_store;
mod run_event_store;
mod run_store;
mod storage_error;
mod stored_message;
mod stored_run;
mod stored_run_event;
mod stored_run_status;
mod task_run_store;
mod usage_store;

pub use conversation_message_store::ConversationMessageStore;
pub use run_event_store::RunEventStore;
pub use run_store::RunStore;
pub use storage_error::StorageError;
pub use stored_message::StoredMessage;
pub use stored_run::StoredRun;
pub use stored_run_event::StoredRunEvent;
pub use stored_run_status::StoredRunStatus;
pub use task_run_store::TaskRunStore;
pub use usage_store::UsageStore;

/// Combined persistence interface required by the run coordinator.
pub trait ExecutionStore:
    RunStore + RunEventStore + ConversationMessageStore + UsageStore + TaskRunStore
{
}
impl<T> ExecutionStore for T where
    T: RunStore + RunEventStore + ConversationMessageStore + UsageStore + TaskRunStore
{
}
