//! File-backed conversation persistence.

mod conversation;
mod conversation_id;
mod conversation_store;
mod conversation_store_error;

fn default_conversation_title() -> String {
    "New conversation".to_owned()
}
#[cfg(test)]
mod tests;

pub use conversation::Conversation;
pub use conversation_id::ConversationId;
pub use conversation_store::ConversationStore;
pub use conversation_store_error::ConversationStoreError;
