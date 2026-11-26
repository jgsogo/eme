//! File-backed conversation persistence and active-conversation index.

use super::{Conversation, ConversationId, ConversationStoreError, default_conversation_title};
use crate::FamilyId;
use agent_runtime::Message;
use chrono::Utc;
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};
use uuid::Uuid;

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
            title: default_conversation_title(),
            archived: false,
            removed_at: None,
            selected_family_id: None,
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
            .map(|id| self.read_conversation(id).and_then(|conversation| {
                if conversation.removed_at.is_some() {
                    Err(ConversationStoreError::NotFound(id))
                } else {
                    Ok(conversation)
                }
            }))
            .transpose()
    }

    /// Lists non-archived conversations belonging to owner, newest activity first.
    pub fn list(&self, owner: &str) -> Result<Vec<Conversation>, ConversationStoreError> {
        self.list_with_archive_state(owner, false)
    }

    /// Lists archived conversations belonging to owner, newest activity first.
    pub fn list_archived(&self, owner: &str) -> Result<Vec<Conversation>, ConversationStoreError> {
        self.list_with_archive_state(owner, true)
    }

    fn list_with_archive_state(
        &self,
        owner: &str,
        archived: bool,
    ) -> Result<Vec<Conversation>, ConversationStoreError> {
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
            if conversation.owner == owner && conversation.archived == archived && conversation.removed_at.is_none() {
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

    /// Loads a conversation by its stable identifier.
    pub fn get(&self, id: ConversationId) -> Result<Conversation, ConversationStoreError> {
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let conversation = self.read_conversation(id)?;
        if conversation.removed_at.is_some() {
            return Err(ConversationStoreError::NotFound(id));
        }
        Ok(conversation)
    }

    /// Changes the display title of a conversation belonging to owner.
    pub fn rename(
        &self,
        owner: &str,
        id: ConversationId,
        title: impl Into<String>,
    ) -> Result<Conversation, ConversationStoreError> {
        let title = title.into();
        let title = title.trim();
        if title.is_empty() {
            return Err(ConversationStoreError::EmptyTitle);
        }

        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut conversation = self.read_conversation(id)?;
        if conversation.owner != owner {
            return Err(ConversationStoreError::OwnerMismatch {
                conversation_id: id,
                owner: owner.to_owned(),
            });
        }

        conversation.title = title.to_owned();
        conversation.updated_at = Utc::now();
        self.write_conversation(&conversation)?;
        Ok(conversation)
    }

    /// Persists the family selected for a conversation owned by the caller.
    pub fn set_selected_family(
        &self,
        owner: &str,
        id: ConversationId,
        family_id: Option<FamilyId>,
    ) -> Result<Conversation, ConversationStoreError> {
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut conversation = self.read_conversation(id)?;
        if conversation.owner != owner {
            return Err(ConversationStoreError::OwnerMismatch {
                conversation_id: id,
                owner: owner.to_owned(),
            });
        }
        conversation.selected_family_id = family_id;
        conversation.updated_at = Utc::now();
        self.write_conversation(&conversation)?;
        Ok(conversation)
    }

    /// Archives or restores a conversation belonging to owner.
    ///
    /// Archiving also clears the active pointer when it refers to this
    /// conversation. The conversation document and its message history remain
    /// untouched and can be restored later.
    pub fn set_archived(
        &self,
        owner: &str,
        id: ConversationId,
        archived: bool,
    ) -> Result<Conversation, ConversationStoreError> {
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut conversation = self.read_conversation(id)?;
        if conversation.owner != owner {
            return Err(ConversationStoreError::OwnerMismatch {
                conversation_id: id,
                owner: owner.to_owned(),
            });
        }

        conversation.archived = archived;
        self.write_conversation(&conversation)?;

        if archived {
            let mut active = self.read_active_index()?;
            if active.get(owner) == Some(&id) {
                active.remove(owner);
                self.write_active_index(&active)?;
            }
        }

        Ok(conversation)
    }

    /// Marks a conversation as removed without deleting its document.
    ///
    /// If the conversation is currently active for that owner, its active
    /// pointer is removed as part of the same process-local critical section.
    pub fn delete(&self, owner: &str, id: ConversationId) -> Result<(), ConversationStoreError> {
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
        if active.get(owner) == Some(&id) {
            active.remove(owner);
            self.write_active_index(&active)?;
        }

        let mut conversation = conversation;
        conversation.removed_at = Some(Utc::now());
        self.write_conversation(&conversation)?;
        Ok(())
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
            Ok(bytes) => {
                let conversation: Conversation = serde_json::from_slice(&bytes)?;
                if conversation.removed_at.is_some() {
                    Err(ConversationStoreError::NotFound(id))
                } else {
                    Ok(conversation)
                }
            },
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
