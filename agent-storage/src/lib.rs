//! Persistent conversation storage and execution persistence contracts.
//!
//! Conversations are stored as JSON documents. The active conversation for
//! each external owner is kept in a separate index. Writes use a temporary
//! file followed by an atomic rename so a process interruption cannot leave a
//! partially-written document.

use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    str::FromStr,
    sync::Mutex,
};

use agent_runtime::Message;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

mod contracts;
mod memory;

pub use contracts::{
    ConversationMessageStore, RunEventStore, RunStore, StorageError, StoredMessage, StoredRun,
    StoredRunEvent, StoredRunStatus,
};
pub use memory::InMemoryStorage;

/// Stable identifier assigned to a logical conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConversationId(Uuid);

impl ConversationId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ConversationId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ConversationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for ConversationId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(value).map(Self)
    }
}

/// A conversation and its complete model-visible message history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: ConversationId,
    /// Stable external identity, for example `telegram:<chat_id>:<user_id>`.
    pub owner: String,
    pub agent: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub messages: Vec<Message>,
}

#[derive(Debug, Error)]
pub enum ConversationStoreError {
    #[error("conversation storage I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("conversation serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("conversation {0} was not found")]
    NotFound(ConversationId),
    #[error("conversation {conversation_id} does not belong to owner '{owner}'")]
    OwnerMismatch {
        conversation_id: ConversationId,
        owner: String,
    },
    #[error("conversation owner must not be empty")]
    EmptyOwner,
    #[error("agent name must not be empty")]
    EmptyAgent,
}

/// File-backed conversation store.
///
/// The store is safe to share between async tasks in one process. It uses a
/// process-local mutex around index and document updates; it is not intended
/// for concurrent writers in multiple server processes.
pub struct ConversationStore {
    root: PathBuf,
    lock: Mutex<()>,
}

impl ConversationStore {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, ConversationStoreError> {
        let root = root.into();
        fs::create_dir_all(root.join("conversations"))?;
        Ok(Self {
            root,
            lock: Mutex::new(()),
        })
    }

    /// Creates a conversation and makes it the active conversation for owner.
    pub fn create(
        &self,
        owner: impl Into<String>,
        agent: impl Into<String>,
    ) -> Result<Conversation, ConversationStoreError> {
        let owner = owner.into();
        let agent = agent.into();
        if owner.trim().is_empty() {
            return Err(ConversationStoreError::EmptyOwner);
        }
        if agent.trim().is_empty() {
            return Err(ConversationStoreError::EmptyAgent);
        }

        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = Utc::now();
        let conversation = Conversation {
            id: ConversationId::new(),
            owner: owner.clone(),
            agent,
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
        };

        self.write_conversation(&conversation)?;
        let mut active = self.read_active_index()?;
        active.insert(owner, conversation.id);
        self.write_active_index(&active)?;

        Ok(conversation)
    }

    /// Returns the active conversation for owner, if one has been created.
    pub fn active(&self, owner: &str) -> Result<Option<Conversation>, ConversationStoreError> {
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let active = self.read_active_index()?;
        active
            .get(owner)
            .copied()
            .map(|id| self.read_conversation(id))
            .transpose()
    }

    /// Lists all conversations belonging to owner, newest activity first.
    pub fn list(&self, owner: &str) -> Result<Vec<Conversation>, ConversationStoreError> {
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let directory = self.root.join("conversations");
        let mut conversations = Vec::new();

        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if !entry.file_type()?.is_file()
                || entry
                    .path()
                    .extension()
                    .and_then(|extension| extension.to_str())
                    != Some("json")
            {
                continue;
            }

            let bytes = fs::read(entry.path())?;
            let conversation: Conversation = serde_json::from_slice(&bytes)?;
            if conversation.owner == owner {
                conversations.push(conversation);
            }
        }

        conversations.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| right.id.to_string().cmp(&left.id.to_string()))
        });
        Ok(conversations)
    }

    /// Selects an existing conversation as the active one for its owner.
    pub fn set_active(
        &self,
        owner: &str,
        id: ConversationId,
    ) -> Result<(), ConversationStoreError> {
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let conversation = self.read_conversation(id)?;
        if conversation.owner != owner {
            return Err(ConversationStoreError::OwnerMismatch {
                conversation_id: id,
                owner: owner.to_owned(),
            });
        }

        let mut active = self.read_active_index()?;
        active.insert(owner.to_owned(), id);
        self.write_active_index(&active)
    }

    /// Moves conversations from a legacy channel-scoped owner to a canonical user owner.
    ///
    /// This is used to migrate Telegram conversations from
    /// `telegram:<chat_id>:<user_id>` to `telegram:<user_id>` without
    /// discarding their history.
    pub fn migrate_owner(
        &self,
        from: &str,
        to: &str,
    ) -> Result<usize, ConversationStoreError> {
        if from.trim().is_empty() || to.trim().is_empty() {
            return Err(ConversationStoreError::EmptyOwner);
        }
        if from == to {
            return Ok(0);
        }

        let _guard = self.lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let directory = self.root.join("conversations");
        let mut migrated = 0;

        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if !entry.file_type()?.is_file()
                || entry.path().extension().and_then(|extension| extension.to_str()) != Some("json")
            {
                continue;
            }
            let bytes = fs::read(entry.path())?;
            let mut conversation: Conversation = serde_json::from_slice(&bytes)?;
            if conversation.owner == from {
                conversation.owner = to.to_owned();
                self.write_conversation(&conversation)?;
                migrated += 1;
            }
        }

        let mut active = self.read_active_index()?;
        if let Some(id) = active.remove(from) {
            active.entry(to.to_owned()).or_insert(id);
            self.write_active_index(&active)?;
        }
        Ok(migrated)
    }

    /// Loads a conversation by its stable identifier.
    pub fn get(&self, id: ConversationId) -> Result<Conversation, ConversationStoreError> {
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.read_conversation(id)
    }

    /// Replaces the complete model-visible history of a conversation.
    pub fn save_messages(
        &self,
        id: ConversationId,
        messages: Vec<Message>,
    ) -> Result<Conversation, ConversationStoreError> {
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut conversation = self.read_conversation(id)?;
        conversation.messages = messages;
        conversation.updated_at = Utc::now();
        self.write_conversation(&conversation)?;
        Ok(conversation)
    }

    fn conversation_path(&self, id: ConversationId) -> PathBuf {
        self.root.join("conversations").join(format!("{id}.json"))
    }

    fn read_conversation(
        &self,
        id: ConversationId,
    ) -> Result<Conversation, ConversationStoreError> {
        let path = self.conversation_path(id);
        match fs::read(path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                Err(ConversationStoreError::NotFound(id))
            }
            Err(error) => Err(error.into()),
        }
    }

    fn write_conversation(
        &self,
        conversation: &Conversation,
    ) -> Result<(), ConversationStoreError> {
        let bytes = serde_json::to_vec_pretty(conversation)?;
        atomic_write(&self.conversation_path(conversation.id), &bytes)?;
        Ok(())
    }

    fn read_active_index(&self) -> Result<HashMap<String, ConversationId>, ConversationStoreError> {
        let path = self.root.join("active-conversations.json");
        match fs::read(path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(HashMap::new()),
            Err(error) => Err(error.into()),
        }
    }

    fn write_active_index(
        &self,
        active: &HashMap<String, ConversationId>,
    ) -> Result<(), ConversationStoreError> {
        let bytes = serde_json::to_vec_pretty(active)?;
        atomic_write(&self.root.join("active-conversations.json"), &bytes)?;
        Ok(())
    }
}

fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), io::Error> {
    let parent = path.parent().expect("storage file always has a parent");
    fs::create_dir_all(parent)?;

    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap().to_string_lossy(),
        Uuid::new_v4()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = (|| {
        file.write_all(contents)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use std::{env, fs, time::SystemTime};

    use agent_runtime::{Message, ToolCall};
    use serde_json::json;

    use super::{ConversationStore, ConversationStoreError};

    fn temporary_directory() -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("agent-storage-test-{unique}"))
    }

    #[test]
    fn creates_active_conversation_and_persists_it_across_reopen() {
        let directory = temporary_directory();
        let store = ConversationStore::open(&directory).unwrap();

        let conversation = store.create("telegram:123:456", "assistant").unwrap();
        assert_eq!(
            store.active("telegram:123:456").unwrap().unwrap().id,
            conversation.id
        );

        drop(store);
        let reopened = ConversationStore::open(&directory).unwrap();
        let loaded = reopened.active("telegram:123:456").unwrap().unwrap();

        assert_eq!(loaded.id, conversation.id);
        assert_eq!(loaded.agent, "assistant");
        assert!(loaded.messages.is_empty());

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn saves_full_runtime_message_history() {
        let directory = temporary_directory();
        let store = ConversationStore::open(&directory).unwrap();
        let conversation = store.create("telegram:123:456", "assistant").unwrap();

        let messages = vec![
            Message::User {
                content: "Calculate 2 + 2".into(),
            },
            Message::Assistant {
                content: None,
                tool_calls: vec![ToolCall {
                    id: "call-1".into(),
                    name: "calculator".into(),
                    arguments: json!({"expression": "2 + 2"}),
                    provider_metadata: Some(json!({"provider": "test"})),
                }],
            },
            Message::Tool {
                tool_call_id: "call-1".into(),
                name: "calculator".into(),
                content: "4".into(),
                is_error: false,
                provider_metadata: None,
            },
        ];

        store
            .save_messages(conversation.id, messages.clone())
            .unwrap();
        let loaded = store.get(conversation.id).unwrap();

        assert_eq!(
            serde_json::to_value(loaded.messages).unwrap(),
            serde_json::to_value(messages).unwrap()
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn lists_only_conversations_for_the_requested_owner() {
        let directory = temporary_directory();
        let store = ConversationStore::open(&directory).unwrap();

        let first = store.create("owner-a", "assistant").unwrap();
        let other = store.create("owner-b", "assistant").unwrap();
        let second = store.create("owner-a", "assistant").unwrap();

        let conversations = store.list("owner-a").unwrap();
        assert_eq!(conversations.len(), 2);
        assert!(conversations.iter().any(|item| item.id == first.id));
        assert!(conversations.iter().any(|item| item.id == second.id));
        assert!(conversations.iter().all(|item| item.owner == "owner-a"));
        assert!(store.list("owner-c").unwrap().is_empty());
        assert_ne!(first.id, other.id);

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn cannot_activate_another_owners_conversation() {
        let directory = temporary_directory();
        let store = ConversationStore::open(&directory).unwrap();
        let conversation = store.create("owner-a", "assistant").unwrap();

        assert!(matches!(
            store.set_active("owner-b", conversation.id),
            Err(ConversationStoreError::OwnerMismatch { .. })
        ));

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn migrates_legacy_owner_and_active_conversation() {
        let directory = temporary_directory();
        let store = ConversationStore::open(&directory).unwrap();
        let conversation = store.create("telegram:123:123", "assistant").unwrap();

        let migrated = store.migrate_owner("telegram:123:123", "telegram:123").unwrap();

        assert_eq!(migrated, 1);
        assert_eq!(store.get(conversation.id).unwrap().owner, "telegram:123");
        assert_eq!(store.active("telegram:123").unwrap().unwrap().id, conversation.id);
        assert!(store.list("telegram:123:123").unwrap().is_empty());

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn new_conversation_replaces_active_without_deleting_previous_history() {
        let directory = temporary_directory();
        let store = ConversationStore::open(&directory).unwrap();

        let first = store.create("telegram:123:456", "assistant").unwrap();
        store
            .save_messages(
                first.id,
                vec![Message::User {
                    content: "first session".into(),
                }],
            )
            .unwrap();

        let second = store.create("telegram:123:456", "assistant").unwrap();

        assert_eq!(
            store.active("telegram:123:456").unwrap().unwrap().id,
            second.id
        );
        assert_eq!(store.get(first.id).unwrap().messages.len(), 1);

        fs::remove_dir_all(directory).unwrap();
    }
}
