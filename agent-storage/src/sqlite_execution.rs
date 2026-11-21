//! SQLite implementation of durable run, event, usage, pricing, and conversation contracts.

mod usage_tests;

use std::{path::Path, str::FromStr, sync::Arc};

use agent_metering::{ModelPricing, UsageRecord};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    ConversationMessageStore, RunEventStore, RunStore, StorageError, StoredMessage, StoredRun,
    StoredRunEvent, UsageStore,
};

#[derive(Clone)]
pub struct SqliteExecutionStore {
    connection: Arc<Mutex<Connection>>,
}

impl SqliteExecutionStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let connection = Connection::open(path).map_err(backend)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(backend)?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
            .map_err(backend)?;
        connection
            .execute_batch(include_str!("../migrations/0007_run_history.sql"))
            .map_err(backend)?;
        connection
            .execute_batch(include_str!("../migrations/0008_usage_metering.sql"))
            .map_err(backend)?;
        ensure_message_column(
            &connection,
            "run_id",
            "ALTER TABLE conversation_messages ADD COLUMN run_id TEXT REFERENCES runs(id)",
        )?;
        ensure_message_column(
            &connection,
            "created_at",
            "ALTER TABLE conversation_messages ADD COLUMN created_at TEXT NOT NULL DEFAULT ''",
        )?;
        Ok(Self { connection: Arc::new(Mutex::new(connection)) })
    }
}

fn backend(error: rusqlite::Error) -> StorageError {
    StorageError::Backend(error.to_string())
}

fn parse_time(value: &str) -> Result<DateTime<Utc>, StorageError> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.with_timezone(&Utc))
        .map_err(|error| StorageError::Backend(error.to_string()))
}

fn ensure_message_column(connection: &Connection, column: &str, alter: &str) -> Result<(), StorageError> {
    let mut statement = connection.prepare("PRAGMA table_info(conversation_messages)").map_err(backend)?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1)).map_err(backend)?;
    for existing in columns {
        if existing.map_err(backend)? == column { return Ok(()); }
    }
    drop(statement);
    connection.execute(alter, []).map_err(backend)?;
    Ok(())
}

#[async_trait]
impl RunStore for SqliteExecutionStore {
    async fn insert_run(&self, run: StoredRun) -> Result<(), StorageError> {
        let connection = self.connection.lock().await;
        let json = serde_json::to_string(&run).map_err(|error| StorageError::Backend(error.to_string()))?;
        connection.execute(
            "INSERT INTO runs(id, conversation_id, created_at, run_json) VALUES (?1, ?2, ?3, ?4)",
            params![run.id.to_string(), run.conversation_id.map(|id| id.to_string()), run.created_at.to_rfc3339(), json],
        ).map_err(|error| match error {
            rusqlite::Error::SqliteFailure(code, _) if code.code == rusqlite::ErrorCode::ConstraintViolation =>
                StorageError::AlreadyExists(format!("run {}", run.id)),
            other => backend(other),
        })?;
        Ok(())
    }

    async fn update_run(&self, run: StoredRun) -> Result<(), StorageError> {
        let connection = self.connection.lock().await;
        let json = serde_json::to_string(&run).map_err(|error| StorageError::Backend(error.to_string()))?;
        let changed = connection.execute(
            "UPDATE runs SET conversation_id = ?1, created_at = ?2, run_json = ?3 WHERE id = ?4",
            params![run.conversation_id.map(|id| id.to_string()), run.created_at.to_rfc3339(), json, run.id.to_string()],
        ).map_err(backend)?;
        if changed == 0 { return Err(StorageError::NotFound(format!("run {}", run.id))); }
        Ok(())
    }

    async fn get_run(&self, id: Uuid) -> Result<Option<StoredRun>, StorageError> {
        let connection = self.connection.lock().await;
        let json: Option<String> = connection.query_row(
            "SELECT run_json FROM runs WHERE id = ?1", [id.to_string()], |row| row.get(0),
        ).optional().map_err(backend)?;
        json.map(|json| serde_json::from_str(&json).map_err(|error| StorageError::Backend(error.to_string()))).transpose()
    }

    async fn list_runs(&self, conversation_id: Option<Uuid>) -> Result<Vec<StoredRun>, StorageError> {
        let connection = self.connection.lock().await;
        let mut statement = connection.prepare(
            "SELECT run_json FROM runs WHERE (?1 IS NULL OR conversation_id = ?1) ORDER BY created_at DESC, id DESC"
        ).map_err(backend)?;
        let rows = statement.query_map([conversation_id.map(|id| id.to_string())], |row| row.get::<_, String>(0)).map_err(backend)?;
        rows.map(|row| {
            let json = row.map_err(backend)?;
            serde_json::from_str(&json).map_err(|error| StorageError::Backend(error.to_string()))
        }).collect()
    }
}

#[async_trait]
impl RunEventStore for SqliteExecutionStore {
    async fn append_event(&self, event: StoredRunEvent) -> Result<(), StorageError> {
        let connection = self.connection.lock().await;
        let exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM runs WHERE id = ?1)", [event.run_id.to_string()], |row| row.get(0),
        ).map_err(backend)?;
        if !exists { return Err(StorageError::NotFound(format!("run {}", event.run_id))); }
        let expected: u64 = connection.query_row(
            "SELECT COUNT(*) FROM run_events WHERE run_id = ?1", [event.run_id.to_string()], |row| row.get(0),
        ).map_err(backend)?;
        if event.sequence < expected {
            return Err(StorageError::DuplicateSequence { stream: format!("run {}", event.run_id), sequence: event.sequence });
        }
        if event.sequence > expected {
            return Err(StorageError::OutOfOrderSequence { stream: format!("run {}", event.run_id), expected, actual: event.sequence });
        }
        let json = serde_json::to_string(&event).map_err(|error| StorageError::Backend(error.to_string()))?;
        connection.execute(
            "INSERT INTO run_events(run_id, sequence, occurred_at, event_json) VALUES (?1, ?2, ?3, ?4)",
            params![event.run_id.to_string(), event.sequence as i64, event.occurred_at.to_rfc3339(), json],
        ).map_err(backend)?;
        Ok(())
    }

    async fn list_events(&self, run_id: Uuid, after_sequence: Option<u64>) -> Result<Vec<StoredRunEvent>, StorageError> {
        let connection = self.connection.lock().await;
        let exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM runs WHERE id = ?1)", [run_id.to_string()], |row| row.get(0),
        ).map_err(backend)?;
        if !exists { return Err(StorageError::NotFound(format!("run {run_id}"))); }
        let mut statement = connection.prepare(
            "SELECT event_json FROM run_events WHERE run_id = ?1 AND (?2 IS NULL OR sequence > ?2) ORDER BY sequence"
        ).map_err(backend)?;
        let rows = statement.query_map(
            params![run_id.to_string(), after_sequence.map(|value| value as i64)],
            |row| row.get::<_, String>(0),
        ).map_err(backend)?;
        rows.map(|row| {
            let json = row.map_err(backend)?;
            serde_json::from_str(&json).map_err(|error| StorageError::Backend(error.to_string()))
        }).collect()
    }
}

#[async_trait]
impl UsageStore for SqliteExecutionStore {
    async fn append_usage(&self, record: UsageRecord) -> Result<(), StorageError> {
        let connection = self.connection.lock().await;
        let id = record.invocation_id.to_string();
        let json = serde_json::to_string(&record).map_err(|error| StorageError::Backend(error.to_string()))?;
        let existing: Option<String> = connection.query_row(
            "SELECT record_json FROM model_usage_records WHERE invocation_id = ?1",
            [&id],
            |row| row.get(0),
        ).optional().map_err(backend)?;
        if let Some(existing) = existing {
            return if existing == json { Ok(()) } else {
                Err(StorageError::AlreadyExists(format!("usage invocation {id}")))
            };
        }
        connection.execute(
            "INSERT INTO model_usage_records(invocation_id, run_id, provider, model, principal_id, occurred_at, record_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, record.run_id.to_string(), record.provider, record.model, record.principal_id, record.occurred_at.to_rfc3339(), json],
        ).map_err(backend)?;
        Ok(())
    }

    async fn list_usage(&self, run_id: Option<Uuid>) -> Result<Vec<UsageRecord>, StorageError> {
        let connection = self.connection.lock().await;
        let mut statement = connection.prepare(
            "SELECT record_json FROM model_usage_records WHERE (?1 IS NULL OR run_id = ?1) ORDER BY occurred_at, invocation_id"
        ).map_err(backend)?;
        let rows = statement.query_map([run_id.map(|id| id.to_string())], |row| row.get::<_, String>(0)).map_err(backend)?;
        rows.map(|row| {
            let json = row.map_err(backend)?;
            serde_json::from_str(&json).map_err(|error| StorageError::Backend(error.to_string()))
        }).collect()
    }

    async fn insert_pricing(&self, pricing: ModelPricing) -> Result<(), StorageError> {
        pricing.validate().map_err(|error| StorageError::Backend(error.to_string()))?;
        let connection = self.connection.lock().await;
        let json = serde_json::to_string(&pricing).map_err(|error| StorageError::Backend(error.to_string()))?;
        connection.execute(
            "INSERT INTO model_pricing(provider, model, version, effective_from, effective_until, pricing_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![pricing.provider, pricing.model, pricing.version, pricing.effective_from.to_rfc3339(), pricing.effective_until.map(|time| time.to_rfc3339()), json],
        ).map_err(|error| match error {
            rusqlite::Error::SqliteFailure(code, _) if code.code == rusqlite::ErrorCode::ConstraintViolation =>
                StorageError::AlreadyExists(format!("pricing {}:{}:{}", pricing.provider, pricing.model, pricing.version)),
            other => backend(other),
        })?;
        Ok(())
    }

    async fn get_pricing(&self, provider: &str, model: &str, version: &str) -> Result<Option<ModelPricing>, StorageError> {
        let connection = self.connection.lock().await;
        let json: Option<String> = connection.query_row(
            "SELECT pricing_json FROM model_pricing WHERE provider = ?1 AND model = ?2 AND version = ?3",
            params![provider, model, version], |row| row.get(0),
        ).optional().map_err(backend)?;
        json.map(|json| serde_json::from_str(&json).map_err(|error| StorageError::Backend(error.to_string()))).transpose()
    }

    async fn list_pricing(&self, provider: &str, model: &str) -> Result<Vec<ModelPricing>, StorageError> {
        let connection = self.connection.lock().await;
        let mut statement = connection.prepare(
            "SELECT pricing_json FROM model_pricing WHERE provider = ?1 AND model = ?2 ORDER BY effective_from, version"
        ).map_err(backend)?;
        let rows = statement.query_map(params![provider, model], |row| row.get::<_, String>(0)).map_err(backend)?;
        rows.map(|row| {
            let json = row.map_err(backend)?;
            serde_json::from_str(&json).map_err(|error| StorageError::Backend(error.to_string()))
        }).collect()
    }
}

#[async_trait]
impl ConversationMessageStore for SqliteExecutionStore {
    async fn append_message(&self, message: StoredMessage) -> Result<(), StorageError> {
        let connection = self.connection.lock().await;
        let conversation_exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM conversations WHERE id = ?1)",
            [message.conversation_id.to_string()], |row| row.get(0),
        ).map_err(backend)?;
        if !conversation_exists { return Err(StorageError::NotFound(format!("conversation {}", message.conversation_id))); }
        if let Some(run_id) = message.run_id {
            let run_conversation: Option<String> = connection.query_row(
                "SELECT conversation_id FROM runs WHERE id = ?1", [run_id.to_string()], |row| row.get(0),
            ).optional().map_err(backend)?;
            match run_conversation {
                None => return Err(StorageError::NotFound(format!("run {run_id}"))),
                Some(id) if id != message.conversation_id.to_string() => return Err(StorageError::InvalidAssociation(format!(
                    "run {run_id} does not belong to conversation {}", message.conversation_id
                ))),
                _ => {}
            }
        }
        let expected: u64 = connection.query_row(
            "SELECT COUNT(*) FROM conversation_messages WHERE conversation_id = ?1",
            [message.conversation_id.to_string()], |row| row.get(0),
        ).map_err(backend)?;
        if message.sequence < expected {
            return Err(StorageError::DuplicateSequence { stream: format!("conversation {}", message.conversation_id), sequence: message.sequence });
        }
        if message.sequence > expected {
            return Err(StorageError::OutOfOrderSequence { stream: format!("conversation {}", message.conversation_id), expected, actual: message.sequence });
        }
        let json = serde_json::to_string(&message.message).map_err(|error| StorageError::Backend(error.to_string()))?;
        connection.execute(
            "INSERT INTO conversation_messages(conversation_id, sequence, message_json, run_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![message.conversation_id.to_string(), message.sequence as i64, json, message.run_id.map(|id| id.to_string()), message.created_at.to_rfc3339()],
        ).map_err(backend)?;
        Ok(())
    }

    async fn list_messages(&self, conversation_id: Uuid) -> Result<Vec<StoredMessage>, StorageError> {
        let connection = self.connection.lock().await;
        let conversation_created: Option<String> = connection.query_row(
            "SELECT created_at FROM conversations WHERE id = ?1", [conversation_id.to_string()], |row| row.get(0),
        ).optional().map_err(backend)?;
        let Some(conversation_created) = conversation_created else {
            return Err(StorageError::NotFound(format!("conversation {conversation_id}")));
        };
        let mut statement = connection.prepare(
            "SELECT sequence, run_id, message_json, created_at FROM conversation_messages WHERE conversation_id = ?1 ORDER BY sequence"
        ).map_err(backend)?;
        let rows = statement.query_map([conversation_id.to_string()], |row| Ok((
            row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?,
        ))).map_err(backend)?;
        rows.map(|row| {
            let (sequence, run_id, json, created_at) = row.map_err(backend)?;
            let run_id = run_id.map(|value| Uuid::from_str(&value).map_err(|error| StorageError::Backend(error.to_string()))).transpose()?;
            let message = serde_json::from_str(&json).map_err(|error| StorageError::Backend(error.to_string()))?;
            let created_at = if created_at.is_empty() { parse_time(&conversation_created)? } else { parse_time(&created_at)? };
            Ok(StoredMessage { conversation_id, run_id, sequence: sequence as u64, message, created_at })
        }).collect()
    }
}
