//! SQLite-backed conversation persistence.
 //!
 //! Conversation metadata and complete structured messages are committed in
 //! transactions. The public API intentionally mirrors the legacy
 //! ConversationStore so callers can be migrated independently.
use std::{path::Path, str::FromStr, sync::{Arc, Mutex, MutexGuard}};

use agent_runtime::Message;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::{Conversation, ConversationId, ConversationStoreError, FamilyId};

#[derive(Clone)]
pub struct SqliteConversationStore {
    connection: Arc<Mutex<Connection>>,
}

impl SqliteConversationStore {
    /// Open (or create) a database and initialize conversation tables.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ConversationStoreError> {
        let connection = Connection::open(path)
            .map_err(|error| ConversationStoreError::Sqlite(error.to_string()))?;
        connection.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|error| ConversationStoreError::Sqlite(error.to_string()))?;
        connection.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
            .map_err(|error| ConversationStoreError::Sqlite(error.to_string()))?;
        connection.execute_batch(include_str!("../migrations/0006_conversations.sql"))
            .map_err(|error| ConversationStoreError::Sqlite(error.to_string()))?;
        Ok(Self { connection: Arc::new(Mutex::new(connection)) })
    }

    pub fn create(
        &self,
        owner: impl Into<String>,
        agent: impl Into<String>,
    ) -> Result<Conversation, ConversationStoreError> {
        let owner = owner.into();
        let agent = agent.into();
        if owner.trim().is_empty() { return Err(ConversationStoreError::EmptyOwner); }
        if agent.trim().is_empty() { return Err(ConversationStoreError::EmptyAgent); }

        let now = Utc::now();
        let conversation = Conversation {
            id: ConversationId::new(),
            owner: owner.clone(),
            agent,
            title: "New conversation".to_owned(),
            selected_family_id: None,
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
        };
        let mut connection = self.lock();
        let tx = connection.transaction().map_err(sqlite_error)?;
        tx.execute(
            "INSERT INTO conversations(id, owner, agent, title, selected_family_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?5)",
            params![conversation.id.to_string(), owner, conversation.agent, conversation.title, now.to_rfc3339()],
        ).map_err(sqlite_error)?;
        tx.execute(
            "INSERT INTO active_conversations(owner, conversation_id) VALUES (?1, ?2) ON CONFLICT(owner) DO UPDATE SET conversation_id = excluded.conversation_id",
            params![conversation.owner, conversation.id.to_string()],
        ).map_err(sqlite_error)?;
        tx.commit().map_err(sqlite_error)?;
        Ok(conversation)
    }

    pub fn active(&self, owner: &str) -> Result<Option<Conversation>, ConversationStoreError> {
        let connection = self.lock();
        let id: Option<String> = connection.query_row(
            "SELECT conversation_id FROM active_conversations WHERE owner = ?1",
            [owner], |row| row.get(0),
        ).optional().map_err(sqlite_error)?;
        id.map(|id| self.load(&connection, ConversationId::from_str(&id).map_err(|e| ConversationStoreError::Sqlite(e.to_string()))?))
            .transpose()
    }

    pub fn list(&self, owner: &str) -> Result<Vec<Conversation>, ConversationStoreError> {
        let connection = self.lock();
        let mut statement = connection.prepare(
            "SELECT id FROM conversations WHERE owner = ?1 ORDER BY updated_at DESC, id DESC"
        ).map_err(sqlite_error)?;
        let ids = statement.query_map([owner], |row| row.get::<_, String>(0))
            .map_err(sqlite_error)?;
        let mut result = Vec::new();
        for id in ids {
            let id = ConversationId::from_str(&id.map_err(sqlite_error)?)
                .map_err(|e| ConversationStoreError::Sqlite(e.to_string()))?;
            result.push(self.load(&connection, id)?);
        }
        Ok(result)
    }

    pub fn set_active(&self, owner: &str, id: ConversationId) -> Result<(), ConversationStoreError> {
        let mut connection = self.lock();
        let tx = connection.transaction().map_err(sqlite_error)?;
        let actual_owner: Option<String> = tx.query_row(
            "SELECT owner FROM conversations WHERE id = ?1",
            [id.to_string()], |row| row.get(0),
        ).optional().map_err(sqlite_error)?;
        match actual_owner {
            None => return Err(ConversationStoreError::NotFound(id)),
            Some(actual) if actual != owner => return Err(ConversationStoreError::OwnerMismatch {
                conversation_id: id, owner: owner.to_owned()
            }),
            _ => {}
        }
        tx.execute(
            "INSERT INTO active_conversations(owner, conversation_id) VALUES (?1, ?2) ON CONFLICT(owner) DO UPDATE SET conversation_id = excluded.conversation_id",
            params![owner, id.to_string()],
        ).map_err(sqlite_error)?;
        tx.commit().map_err(sqlite_error)
    }

    pub fn get(&self, id: ConversationId) -> Result<Conversation, ConversationStoreError> {
        self.load(&self.lock(), id)
    }

    pub fn rename(
        &self, owner: &str, id: ConversationId, title: impl Into<String>,
    ) -> Result<Conversation, ConversationStoreError> {
        let title = title.into();
        let title = title.trim();
        if title.is_empty() { return Err(ConversationStoreError::EmptyTitle); }
        self.update_owned(owner, id, |connection| {
            connection.execute(
                "UPDATE conversations SET title = ?1, updated_at = ?2 WHERE id = ?3",
                params![title, Utc::now().to_rfc3339(), id.to_string()],
            ).map_err(sqlite_error)?;
            Ok(())
        })?;
        self.get(id)
    }

    pub fn set_selected_family(
        &self, owner: &str, id: ConversationId, family_id: Option<FamilyId>,
    ) -> Result<Conversation, ConversationStoreError> {
        self.update_owned(owner, id, |connection| {
            connection.execute(
                "UPDATE conversations SET selected_family_id = ?1, updated_at = ?2 WHERE id = ?3",
                params![family_id.map(|id| id.to_string()), Utc::now().to_rfc3339(), id.to_string()],
            ).map_err(sqlite_error)?;
            Ok(())
        })?;
        self.get(id)
    }

    pub fn delete(&self, owner: &str, id: ConversationId) -> Result<(), ConversationStoreError> {
        let mut connection = self.lock();
        let tx = connection.transaction().map_err(sqlite_error)?;
        let actual_owner: Option<String> = tx.query_row(
            "SELECT owner FROM conversations WHERE id = ?1", [id.to_string()],
            |row| row.get(0),
        ).optional().map_err(sqlite_error)?;
        match actual_owner {
            None => return Err(ConversationStoreError::NotFound(id)),
            Some(actual) if actual != owner => return Err(ConversationStoreError::OwnerMismatch {
                conversation_id: id, owner: owner.to_owned()
            }),
            _ => {}
        }
        tx.execute("DELETE FROM active_conversations WHERE owner = ?1 AND conversation_id = ?2",
            params![owner, id.to_string()]).map_err(sqlite_error)?;
        tx.execute("DELETE FROM conversations WHERE id = ?1", [id.to_string()])
            .map_err(sqlite_error)?;
        tx.commit().map_err(sqlite_error)
    }

    /// Replaces the full message history. New execution paths should append
    /// messages incrementally through the message repository introduced later.
    pub fn save_messages(
        &self, id: ConversationId, messages: Vec<Message>,
    ) -> Result<Conversation, ConversationStoreError> {
        let mut connection = self.lock();
        let tx = connection.transaction().map_err(sqlite_error)?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM conversations WHERE id = ?1)",
            [id.to_string()], |row| row.get(0),
        ).map_err(sqlite_error)?;
        if !exists { return Err(ConversationStoreError::NotFound(id)); }
        tx.execute("DELETE FROM conversation_messages WHERE conversation_id = ?1",
            [id.to_string()]).map_err(sqlite_error)?;
        for (sequence, message) in messages.iter().enumerate() {
            let json = serde_json::to_string(message)?;
            tx.execute(
                "INSERT INTO conversation_messages(conversation_id, sequence, message_json) VALUES (?1, ?2, ?3)",
                params![id.to_string(), sequence as i64, json],
            ).map_err(sqlite_error)?;
        }
        tx.execute("UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), id.to_string()]).map_err(sqlite_error)?;
        tx.commit().map_err(sqlite_error)?;
        self.get(id)
    }

    fn update_owned<F>(&self, owner: &str, id: ConversationId, update: F) -> Result<(), ConversationStoreError>
    where F: FnOnce(&Connection) -> Result<(), ConversationStoreError> {
        let connection = self.lock();
        let actual_owner: Option<String> = connection.query_row(
            "SELECT owner FROM conversations WHERE id = ?1", [id.to_string()],
            |row| row.get(0),
        ).optional().map_err(sqlite_error)?;
        match actual_owner {
            None => Err(ConversationStoreError::NotFound(id)),
            Some(actual) if actual != owner => Err(ConversationStoreError::OwnerMismatch {
                conversation_id: id, owner: owner.to_owned()
            }),
            _ => update(&connection),
        }
    }

    fn load(&self, connection: &Connection, id: ConversationId) -> Result<Conversation, ConversationStoreError> {
        let row = connection.query_row(
            "SELECT owner, agent, title, selected_family_id, created_at, updated_at FROM conversations WHERE id = ?1",
            [id.to_string()],
            |row| Ok((
                row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?, row.get::<_, String>(4)?, row.get::<_, String>(5)?,
            )),
        ).optional().map_err(sqlite_error)?;
        let Some((owner, agent, title, family, created, updated)) = row else {
            return Err(ConversationStoreError::NotFound(id));
        };
        let mut statement = connection.prepare(
            "SELECT message_json FROM conversation_messages WHERE conversation_id = ?1 ORDER BY sequence ASC"
        ).map_err(sqlite_error)?;
        let rows = statement.query_map([id.to_string()], |row| row.get::<_, String>(0))
            .map_err(sqlite_error)?;
        let mut messages = Vec::new();
        for row in rows {
            messages.push(serde_json::from_str(&row.map_err(sqlite_error)?)?);
        }
        Ok(Conversation {
            id, owner, agent, title,
            selected_family_id: family.map(|value| FamilyId::from_str(&value)
                .map_err(|error| ConversationStoreError::Sqlite(error.to_string())))
                .transpose()?,
            created_at: parse_time(&created)?, updated_at: parse_time(&updated)?, messages,
        })
    }

    fn lock(&self) -> MutexGuard<'_, Connection> {
        self.connection.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
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
