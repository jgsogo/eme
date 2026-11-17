use std::{path::Path, str::FromStr, sync::Arc};

use agent_storage::{FamilyId, FamilyStore, UserId};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{NewNote, Note, NoteSummary, NotesError, UpdateNote};

#[derive(Clone)]
pub struct NotesStore {
    connection: Arc<Mutex<Connection>>,
    families: Arc<dyn FamilyStore>,
}

impl NotesStore {
    pub fn open(
        path: impl AsRef<Path>,
        families: Arc<dyn FamilyStore>,
    ) -> Result<Self, NotesError> {
        let connection = Connection::open(path).map_err(storage)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(storage)?;
        connection
            .execute_batch(include_str!("../../migrations/0001_notes.sql"))
            .map_err(storage)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            families,
        })
    }

    pub async fn create(&self, note: NewNote) -> Result<Note, NotesError> {
        let owner = parse_user(&note.owner_id)?;
        if note.title.trim().is_empty() {
            return Err(NotesError::Invalid("title must not be empty".into()));
        }
        if let Some(id) = &note.family_id {
            self.require_family_member(owner, id).await?;
        }
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let connection = self.connection.lock().await;
        connection.execute(
            "INSERT INTO notes(id, owner_id, family_id, title, content, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![id, note.owner_id, note.family_id, note.title.trim(), note.content, now],
        ).map_err(storage)?;
        load_note(&connection, &id)?.ok_or(NotesError::NotFound)
    }

    pub async fn list_accessible(
        &self,
        user_id: &str,
        selected_family: Option<&str>,
    ) -> Result<Vec<NoteSummary>, NotesError> {
        let user = parse_user(user_id)?;
        let family_ids = self.accessible_families(user, selected_family).await?;
        let connection = self.connection.lock().await;
        let mut result = Vec::new();
        let mut statement = connection.prepare("SELECT id, owner_id, family_id, title, created_at, updated_at FROM notes WHERE owner_id = ?1 ORDER BY updated_at DESC, id").map_err(storage)?;
        let rows = statement
            .query_map([user_id], summary_row)
            .map_err(storage)?;
        for row in rows {
            result.push(row.map_err(storage)?);
        }
        drop(statement);
        for family_id in family_ids {
            let mut statement = connection.prepare("SELECT id, owner_id, family_id, title, created_at, updated_at FROM notes WHERE family_id = ?1 ORDER BY updated_at DESC, id").map_err(storage)?;
            let rows = statement
                .query_map([family_id.to_string()], summary_row)
                .map_err(storage)?;
            for row in rows {
                result.push(row.map_err(storage)?);
            }
        }
        result.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        result.dedup_by(|a, b| a.id == b.id);
        Ok(result)
    }

    pub async fn get_accessible(
        &self,
        user_id: &str,
        selected_family: Option<&str>,
        note_id: &str,
    ) -> Result<Note, NotesError> {
        let user = parse_user(user_id)?;
        let family_ids = self.accessible_families(user, selected_family).await?;
        let connection = self.connection.lock().await;
        let note = load_note(&connection, note_id)?.ok_or(NotesError::NotFound)?;
        if note.owner_id != user_id
            && !note
                .family_id
                .as_ref()
                .is_some_and(|id| family_ids.iter().any(|family| family.to_string() == *id))
        {
            return Err(NotesError::NotFound);
        }
        Ok(note)
    }

    pub async fn update_owned(
        &self,
        user_id: &str,
        note_id: &str,
        update: UpdateNote,
    ) -> Result<Note, NotesError> {
        let user = parse_user(user_id)?;
        if let Some(Some(family_id)) = &update.family_id {
            self.require_family_member(user, family_id).await?;
        }
        let connection = self.connection.lock().await;
        let mut note = load_note(&connection, note_id)?
            .filter(|note| note.owner_id == user_id)
            .ok_or(NotesError::NotFound)?;
        if let Some(title) = update.title {
            if title.trim().is_empty() {
                return Err(NotesError::Invalid("title must not be empty".into()));
            }
            note.title = title.trim().to_owned();
        }
        if let Some(content) = update.content {
            note.content = content;
        }
        if let Some(family_id) = update.family_id {
            note.family_id = family_id;
        }
        note.updated_at = Utc::now();
        connection.execute("UPDATE notes SET family_id=?1, title=?2, content=?3, updated_at=?4 WHERE id=?5 AND owner_id=?6",
            params![note.family_id, note.title, note.content, note.updated_at.to_rfc3339(), note_id, user_id]).map_err(storage)?;
        Ok(note)
    }

    pub async fn delete_owned(&self, user_id: &str, note_id: &str) -> Result<(), NotesError> {
        parse_user(user_id)?;
        let connection = self.connection.lock().await;
        let changed = connection
            .execute(
                "DELETE FROM notes WHERE id=?1 AND owner_id=?2",
                params![note_id, user_id],
            )
            .map_err(storage)?;
        if changed == 0 {
            return Err(NotesError::NotFound);
        }
        Ok(())
    }

    async fn require_family_member(&self, user: UserId, value: &str) -> Result<(), NotesError> {
        let id = FamilyId::from_str(value)
            .map_err(|_| NotesError::Invalid("invalid family id".into()))?;
        let families = self
            .families
            .list_families(user)
            .await
            .map_err(|e| NotesError::Family(e.to_string()))?;
        if families.iter().any(|family| family.id == id) {
            Ok(())
        } else {
            Err(NotesError::NotFound)
        }
    }

    async fn accessible_families(
        &self,
        user: UserId,
        selected: Option<&str>,
    ) -> Result<Vec<FamilyId>, NotesError> {
        let families = self
            .families
            .list_families(user)
            .await
            .map_err(|e| NotesError::Family(e.to_string()))?;
        if let Some(selected) = selected {
            let id = FamilyId::from_str(selected)
                .map_err(|_| NotesError::Invalid("invalid family id in trusted context".into()))?;
            if !families.iter().any(|family| family.id == id) {
                return Err(NotesError::NotFound);
            }
            Ok(vec![id])
        } else {
            Ok(families.into_iter().map(|family| family.id).collect())
        }
    }
}

fn summary_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<NoteSummary> {
    Ok(NoteSummary {
        id: row.get(0)?,
        owner_id: row.get(1)?,
        family_id: row.get(2)?,
        title: row.get(3)?,
        created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
            .map(|v| v.with_timezone(&Utc))
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    4,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?,
        updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
            .map(|v| v.with_timezone(&Utc))
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    5,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?,
    })
}
fn parse_user(value: &str) -> Result<UserId, NotesError> {
    UserId::from_str(value).map_err(|_| NotesError::Invalid("invalid user id".into()))
}
fn storage(error: rusqlite::Error) -> NotesError {
    NotesError::Storage(error.to_string())
}
fn parse_time(value: &str) -> Result<DateTime<Utc>, NotesError> {
    DateTime::parse_from_rfc3339(value)
        .map(|v| v.with_timezone(&Utc))
        .map_err(|e| NotesError::Storage(e.to_string()))
}
fn load_note(connection: &Connection, id: &str) -> Result<Option<Note>, NotesError> {
    connection.query_row("SELECT id, owner_id, family_id, title, content, created_at, updated_at FROM notes WHERE id=?1", [id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<String>>(2)?, row.get::<_, String>(3)?, row.get::<_, String>(4)?, row.get::<_, String>(5)?, row.get::<_, String>(6)?))
    }).optional().map_err(storage)?.map(|(id, owner_id, family_id, title, content, created_at, updated_at)| Ok(Note {
        id, owner_id, family_id, title, content, created_at: parse_time(&created_at)?, updated_at: parse_time(&updated_at)?
    })).transpose()
}
