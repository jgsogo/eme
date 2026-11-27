//! SQLite-backed conversation persistence.
//!
//! Conversation metadata and complete structured messages are committed in
//! transactions. The public API intentionally mirrors the legacy
//! ConversationStore so callers can be migrated independently.

use std::{
    collections::HashMap,
    fs,
    path::Path,
    str::FromStr,
    sync::{Arc, Mutex, MutexGuard},
};

use agent_runtime::Message;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};

use crate::{Conversation, ConversationId, ConversationStoreError, FamilyId};

#[derive(Clone)]
pub struct SqliteConversationStore {
    connection: Arc<Mutex<Connection>>,
}

impl SqliteConversationStore {
    /// Open (or create) a database and initialize conversation tables.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ConversationStoreError> {
        let connection = Connection::open(path).map_err(sqlite_error)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(sqlite_error)?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
            .map_err(sqlite_error)?;
        connection
            .execute_batch(include_str!("../migrations/0006_conversations.sql"))
            .map_err(sqlite_error)?;
        let has_removed_at = {
            let mut statement = connection
                .prepare("PRAGMA table_info(conversations)")
                .map_err(sqlite_error)?;
            let columns = statement
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(sqlite_error)?;
            columns
                .collect::<Result<Vec<_>, _>>()
                .map_err(sqlite_error)?
                .iter()
                .any(|name| name == "removed_at")
        };
        if !has_removed_at {
            connection
                .execute_batch(include_str!(
                    "../migrations/0010_soft_delete_conversations.sql"
                ))
                .map_err(sqlite_error)?;
        }
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

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

        let now = Utc::now();
        let conversation = Conversation {
            id: ConversationId::new(),
            owner: owner.clone(),
            agent,
            title: "New conversation".to_owned(),
            archived: false,
            selected_family_id: None,
            removed_at: None,
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
        };
        let mut connection = self.lock();
        let tx = connection.transaction().map_err(sqlite_error)?;
        tx.execute(
            "INSERT INTO conversations(id, owner, agent, title, archived, selected_family_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 0, NULL, ?5, ?5)",
            params![
                conversation.id.to_string(),
                owner,
                conversation.agent,
                conversation.title,
                now.to_rfc3339()
            ],
        )
        .map_err(sqlite_error)?;
        tx.execute(
            "INSERT INTO active_conversations(owner, conversation_id) VALUES (?1, ?2) ON CONFLICT(owner) DO UPDATE SET conversation_id = excluded.conversation_id",
            params![conversation.owner, conversation.id.to_string()],
        )
        .map_err(sqlite_error)?;
        tx.commit().map_err(sqlite_error)?;
        Ok(conversation)
    }

    pub fn active(&self, owner: &str) -> Result<Option<Conversation>, ConversationStoreError> {
        let connection = self.lock();
        let id: Option<String> = connection
            .query_row(
                "SELECT a.conversation_id FROM active_conversations a JOIN conversations c ON c.id = a.conversation_id WHERE a.owner = ?1 AND c.removed_at IS NULL",
                [owner],
                |row| row.get(0),
            )
            .optional()
            .map_err(sqlite_error)?;
        id.map(|id| {
            self.load(
                &connection,
                ConversationId::from_str(&id)
                    .map_err(|error| ConversationStoreError::Sqlite(error.to_string()))?,
            )
        })
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
        let connection = self.lock();
        let mut statement = connection
            .prepare(
                "SELECT id FROM conversations WHERE owner = ?1 AND archived = ?2 AND removed_at IS NULL ORDER BY updated_at DESC, id DESC",
            )
            .map_err(sqlite_error)?;
        let ids = statement
            .query_map(params![owner, archived], |row| row.get::<_, String>(0))
            .map_err(sqlite_error)?;
        let mut result = Vec::new();
        for id in ids {
            let id = ConversationId::from_str(&id.map_err(sqlite_error)?)
                .map_err(|error| ConversationStoreError::Sqlite(error.to_string()))?;
            result.push(self.load(&connection, id)?);
        }
        Ok(result)
    }

    pub fn set_active(
        &self,
        owner: &str,
        id: ConversationId,
    ) -> Result<(), ConversationStoreError> {
        let mut connection = self.lock();
        let tx = connection.transaction().map_err(sqlite_error)?;
        let actual_owner: Option<String> = tx
            .query_row(
                "SELECT owner FROM conversations WHERE id = ?1 AND removed_at IS NULL",
                [id.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(sqlite_error)?;
        match actual_owner {
            None => return Err(ConversationStoreError::NotFound(id)),
            Some(actual) if actual != owner => {
                return Err(ConversationStoreError::OwnerMismatch {
                    conversation_id: id,
                    owner: owner.to_owned(),
                });
            }
            _ => {}
        }
        tx.execute(
            "INSERT INTO active_conversations(owner, conversation_id) VALUES (?1, ?2) ON CONFLICT(owner) DO UPDATE SET conversation_id = excluded.conversation_id",
            params![owner, id.to_string()],
        )
        .map_err(sqlite_error)?;
        tx.commit().map_err(sqlite_error)
    }

    pub fn get(&self, id: ConversationId) -> Result<Conversation, ConversationStoreError> {
        let conversation = self.load(&self.lock(), id)?;
        if conversation.removed_at.is_some() {
            return Err(ConversationStoreError::NotFound(id));
        }
        Ok(conversation)
    }

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
        self.update_owned(owner, id, |connection| {
            connection
                .execute(
                    "UPDATE conversations SET title = ?1, updated_at = ?2 WHERE id = ?3",
                    params![title, Utc::now().to_rfc3339(), id.to_string()],
                )
                .map_err(sqlite_error)?;
            Ok(())
        })?;
        self.get(id)
    }

    pub fn set_selected_family(
        &self,
        owner: &str,
        id: ConversationId,
        family_id: Option<FamilyId>,
    ) -> Result<Conversation, ConversationStoreError> {
        self.update_owned(owner, id, |connection| {
            connection
                .execute(
                    "UPDATE conversations SET selected_family_id = ?1, updated_at = ?2 WHERE id = ?3",
                    params![
                        family_id.map(|id| id.to_string()),
                        Utc::now().to_rfc3339(),
                        id.to_string()
                    ],
                )
                .map_err(sqlite_error)?;
            Ok(())
        })?;
        self.get(id)
    }

    /// Archives or restores a conversation belonging to owner.
    ///
    /// Archiving also clears the active pointer when it refers to this
    /// conversation. The conversation and its message history remain intact.
    pub fn set_archived(
        &self,
        owner: &str,
        id: ConversationId,
        archived: bool,
    ) -> Result<Conversation, ConversationStoreError> {
        let mut connection = self.lock();
        let tx = connection.transaction().map_err(sqlite_error)?;
        let actual_owner: Option<String> = tx
            .query_row(
                "SELECT owner FROM conversations WHERE id = ?1 AND removed_at IS NULL",
                [id.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(sqlite_error)?;
        match actual_owner {
            None => return Err(ConversationStoreError::NotFound(id)),
            Some(actual) if actual != owner => {
                return Err(ConversationStoreError::OwnerMismatch {
                    conversation_id: id,
                    owner: owner.to_owned(),
                });
            }
            _ => {}
        }

        tx.execute(
            "UPDATE conversations SET archived = ?1, updated_at = ?2 WHERE id = ?3",
            params![archived, Utc::now().to_rfc3339(), id.to_string()],
        )
        .map_err(sqlite_error)?;
        if archived {
            tx.execute(
                "DELETE FROM active_conversations WHERE owner = ?1 AND conversation_id = ?2",
                params![owner, id.to_string()],
            )
            .map_err(sqlite_error)?;
        }
        tx.commit().map_err(sqlite_error)?;
        self.get(id)
    }

    pub fn delete(&self, owner: &str, id: ConversationId) -> Result<(), ConversationStoreError> {
        let mut connection = self.lock();
        let tx = connection.transaction().map_err(sqlite_error)?;
        let actual_owner: Option<String> = tx
            .query_row(
                "SELECT owner FROM conversations WHERE id = ?1 AND removed_at IS NULL",
                [id.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(sqlite_error)?;
        match actual_owner {
            None => return Err(ConversationStoreError::NotFound(id)),
            Some(actual) if actual != owner => {
                return Err(ConversationStoreError::OwnerMismatch {
                    conversation_id: id,
                    owner: owner.to_owned(),
                });
            }
            _ => {}
        }
        tx.execute(
            "DELETE FROM active_conversations WHERE owner = ?1 AND conversation_id = ?2",
            params![owner, id.to_string()],
        )
        .map_err(sqlite_error)?;
        tx.execute(
            "UPDATE conversations SET removed_at = ?1 WHERE id = ?2 AND removed_at IS NULL",
            params![Utc::now().to_rfc3339(), id.to_string()],
        )
        .map_err(sqlite_error)?;
        tx.commit().map_err(sqlite_error)
    }

    /// Replaces the full message history. New execution paths should append
    /// messages incrementally through the message repository introduced later.
    pub fn save_messages(
        &self,
        id: ConversationId,
        messages: Vec<Message>,
    ) -> Result<Conversation, ConversationStoreError> {
        let mut connection = self.lock();
        let tx = connection.transaction().map_err(sqlite_error)?;
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM conversations WHERE id = ?1 AND removed_at IS NULL)",
                [id.to_string()],
                |row| row.get(0),
            )
            .map_err(sqlite_error)?;
        if !exists {
            return Err(ConversationStoreError::NotFound(id));
        }
        tx.execute(
            "DELETE FROM conversation_messages WHERE conversation_id = ?1",
            [id.to_string()],
        )
        .map_err(sqlite_error)?;
        for (sequence, message) in messages.iter().enumerate() {
            let json = serde_json::to_string(message)?;
            tx.execute(
                "INSERT INTO conversation_messages(conversation_id, sequence, message_json) VALUES (?1, ?2, ?3)",
                params![id.to_string(), sequence as i64, json],
            )
            .map_err(sqlite_error)?;
        }
        tx.execute(
            "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), id.to_string()],
        )
        .map_err(sqlite_error)?;
        tx.commit().map_err(sqlite_error)?;
        self.get(id)
    }

    /// Import the legacy file store without changing or deleting its files.
    ///
    /// Existing SQLite rows always win, making repeated or interrupted imports
    /// safe. Each conversation and its ordered messages are inserted atomically.
    pub fn import_legacy(
        &self,
        legacy_root: impl AsRef<Path>,
    ) -> Result<LegacyImportReport, ConversationStoreError> {
        let legacy_root = legacy_root.as_ref();
        let conversations_dir = legacy_root.join("conversations");
        let mut report = LegacyImportReport::default();

        if conversations_dir.exists() {
            let mut paths = fs::read_dir(&conversations_dir)?
                .map(|entry| entry.map(|entry| entry.path()))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .filter(|path| {
                    path.is_file()
                        && path.extension().and_then(|value| value.to_str()) == Some("json")
                })
                .collect::<Vec<_>>();
            paths.sort();

            for path in paths {
                let parsed = fs::read(&path)
                    .map_err(ConversationStoreError::Io)
                    .and_then(|bytes| {
                        serde_json::from_slice::<Conversation>(&bytes)
                            .map_err(ConversationStoreError::Serialization)
                    });
                let conversation = match parsed {
                    Ok(conversation) => conversation,
                    Err(error) => {
                        report
                            .invalid_documents
                            .push(format!("{}: {error}", path.display()));
                        continue;
                    }
                };
                match self.import_conversation(&conversation) {
                    Ok(true) => report.imported += 1,
                    Ok(false) => report.already_present += 1,
                    Err(error) => report
                        .invalid_documents
                        .push(format!("{}: {error}", path.display())),
                }
            }
        }

        let active_path = legacy_root.join("active-conversations.json");
        if active_path.exists() {
            let bytes = fs::read(&active_path)?;
            let active: HashMap<String, ConversationId> = serde_json::from_slice(&bytes)?;
            let connection = self.lock();
            for (owner, id) in active {
                let actual_owner: Option<String> = connection
                    .query_row(
                        "SELECT owner FROM conversations WHERE id = ?1 AND removed_at IS NULL",
                        [id.to_string()],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(sqlite_error)?;
                match actual_owner {
                    Some(actual) if actual == owner => {
                        let inserted = connection
                            .execute(
                                "INSERT OR IGNORE INTO active_conversations(owner, conversation_id) VALUES (?1, ?2)",
                                params![owner, id.to_string()],
                            )
                            .map_err(sqlite_error)?;
                        report.active_mappings_imported += inserted;
                    }
                    Some(_) => report
                        .invalid_active_entries
                        .push(format!("owner mapping does not match conversation {id}")),
                    None => report
                        .invalid_active_entries
                        .push(format!("active conversation {id} does not exist")),
                }
            }
        }
        Ok(report)
    }

    fn import_conversation(
        &self,
        conversation: &Conversation,
    ) -> Result<bool, ConversationStoreError> {
        let mut connection = self.lock();
        let tx = connection.transaction().map_err(sqlite_error)?;
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM conversations WHERE id = ?1)",
                [conversation.id.to_string()],
                |row| row.get(0),
            )
            .map_err(sqlite_error)?;
        if exists {
            tx.rollback().map_err(sqlite_error)?;
            return Ok(false);
        }
        tx.execute(
            "INSERT INTO conversations(id, owner, agent, title, archived, selected_family_id, created_at, updated_at, removed_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                conversation.id.to_string(),
                conversation.owner,
                conversation.agent,
                conversation.title,
                conversation.archived,
                conversation.selected_family_id.map(|id| id.to_string()),
                conversation.created_at.to_rfc3339(),
                conversation.updated_at.to_rfc3339(),
                conversation.removed_at.map(|value| value.to_rfc3339())
            ],
        )
        .map_err(sqlite_error)?;
        for (sequence, message) in conversation.messages.iter().enumerate() {
            tx.execute(
                "INSERT INTO conversation_messages(conversation_id, sequence, message_json) VALUES (?1, ?2, ?3)",
                params![
                    conversation.id.to_string(),
                    sequence as i64,
                    serde_json::to_string(message)?
                ],
            )
            .map_err(sqlite_error)?;
        }
        tx.commit().map_err(sqlite_error)?;
        Ok(true)
    }

    fn update_owned<F>(
        &self,
        owner: &str,
        id: ConversationId,
        update: F,
    ) -> Result<(), ConversationStoreError>
    where
        F: FnOnce(&Connection) -> Result<(), ConversationStoreError>,
    {
        let connection = self.lock();
        let actual_owner: Option<String> = connection
            .query_row(
                "SELECT owner FROM conversations WHERE id = ?1 AND removed_at IS NULL",
                [id.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(sqlite_error)?;
        match actual_owner {
            None => Err(ConversationStoreError::NotFound(id)),
            Some(actual) if actual != owner => Err(ConversationStoreError::OwnerMismatch {
                conversation_id: id,
                owner: owner.to_owned(),
            }),
            _ => update(&connection),
        }
    }

    fn load(
        &self,
        connection: &Connection,
        id: ConversationId,
    ) -> Result<Conversation, ConversationStoreError> {
        let row = connection
            .query_row(
                "SELECT owner, agent, title, archived, selected_family_id, created_at, updated_at, removed_at FROM conversations WHERE id = ?1",
                [id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, bool>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, Option<String>>(7)?,
                    ))
                },
            )
            .optional()
            .map_err(sqlite_error)?;
        let Some((owner, agent, title, archived, family, created, updated, removed_at)) = row
        else {
            return Err(ConversationStoreError::NotFound(id));
        };
        let mut statement = connection
            .prepare(
                "SELECT message_json FROM conversation_messages WHERE conversation_id = ?1 ORDER BY sequence ASC",
            )
            .map_err(sqlite_error)?;
        let rows = statement
            .query_map([id.to_string()], |row| row.get::<_, String>(0))
            .map_err(sqlite_error)?;
        let mut messages = Vec::new();
        for row in rows {
            messages.push(serde_json::from_str(&row.map_err(sqlite_error)?)?);
        }
        Ok(Conversation {
            id,
            owner,
            agent,
            title,
            archived,
            selected_family_id: family
                .map(|value| {
                    FamilyId::from_str(&value)
                        .map_err(|error| ConversationStoreError::Sqlite(error.to_string()))
                })
                .transpose()?,
            created_at: parse_time(&created)?,
            updated_at: parse_time(&updated)?,
            removed_at: removed_at.map(|value| parse_time(&value)).transpose()?,
            messages,
        })
    }

    fn lock(&self) -> MutexGuard<'_, Connection> {
        self.connection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn sqlite_error(error: rusqlite::Error) -> ConversationStoreError {
    ConversationStoreError::Sqlite(error.to_string())
}

fn parse_time(value: &str) -> Result<DateTime<Utc>, ConversationStoreError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| ConversationStoreError::Sqlite(error.to_string()))
}

/// Summary of a legacy conversation import. Problems are reported by path or
/// mapping; callers must not silently start serving when either list is non-empty.
#[derive(Debug, Default)]
pub struct LegacyImportReport {
    pub imported: usize,
    pub already_present: usize,
    pub active_mappings_imported: usize,
    pub invalid_documents: Vec<String>,
    pub invalid_active_entries: Vec<String>,
}

#[cfg(test)]
mod tests;
