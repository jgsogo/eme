//! File-backed conversation persistence.

#[path = "conversation/conversation.rs"]
mod conversation_record;
mod conversation_id;
mod conversation_store;
mod conversation_store_error;
#[cfg(test)]
mod tests;

pub use conversation_record::Conversation;
pub use conversation_id::ConversationId;
pub use conversation_store::ConversationStore;
pub use conversation_store_error::ConversationStoreError;

fn default_conversation_title() -> String {
    "New conversation".to_owned()
}
