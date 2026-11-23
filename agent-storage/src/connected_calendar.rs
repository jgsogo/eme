//! Persistence for calendars discovered through external provider connections.
use crate::{ExternalConnectionId, FamilyId, UserId};
use async_trait::async_trait;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{path::Path, str::FromStr};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConnectedCalendarId(Uuid);

impl ConnectedCalendarId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}
impl Default for ConnectedCalendarId {
    fn default() -> Self {
        Self::new()
    }
}
impl std::fmt::Display for ConnectedCalendarId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl FromStr for ConnectedCalendarId {
    type Err = uuid::Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(value).map(Self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarAccessMode {
    ReadOnly,
    ReadWrite,
}
impl CalendarAccessMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::ReadWrite => "read_write",
        }
    }
    fn parse(value: &str) -> Result<Self, ConnectedCalendarStorageError> {
        match value {
            "read_only" => Ok(Self::ReadOnly),
            "read_write" => Ok(Self::ReadWrite),
            _ => Err(ConnectedCalendarStorageError::Backend(format!(
                "invalid calendar access mode: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarFamilyAccess {
    Viewer,
    Contributor,
}
impl CalendarFamilyAccess {
    fn as_str(self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Contributor => "contributor",
        }
    }
    fn parse(value: &str) -> Result<Self, ConnectedCalendarStorageError> {
        match value {
            "viewer" => Ok(Self::Viewer),
            "contributor" => Ok(Self::Contributor),
            _ => Err(ConnectedCalendarStorageError::Backend(format!(
                "invalid calendar family access: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredCalendar {
    pub provider_calendar_id: String,
    pub display_name: Option<String>,
    pub time_zone: Option<String>,
    pub provider_access_role: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectedCalendar {
    pub id: ConnectedCalendarId,
    pub owner_id: UserId,
    pub connection_id: ExternalConnectionId,
    /// Provider IDs are server-side routing details, never part of HTTP responses.
    #[serde(skip_serializing)]
    pub provider_calendar_id: String,
    pub display_name: Option<String>,
    pub time_zone: Option<String>,
    pub provider_access_role: Option<String>,
    pub selected: bool,
    pub access_mode: CalendarAccessMode,
    pub family_id: Option<FamilyId>,
    pub family_access: Option<CalendarFamilyAccess>,
}

#[derive(Debug, Error)]
pub enum ConnectedCalendarStorageError {
    #[error("active external connection was not found")]
    ConnectionNotFound,
    #[error("connected calendar was not found")]
    CalendarNotFound,
    #[error("provider calendar ID must not be empty")]
    EmptyProviderCalendarId,
    #[error("Google account has not granted calendar write scope")]
    InsufficientScopes,
    #[error("calendar must be selected before it can be shared")]
    FamilyShareRequiresSelected,
    #[error("contributor sharing requires owner read/write access")]
    FamilyWriteRequiresOwnerWrite,
    #[error("connected calendar storage backend failed: {0}")]
    Backend(String),
}

#[async_trait]
pub trait ConnectedCalendarStore: Send + Sync {
    async fn sync_discovered(
        &self,
        owner: UserId,
        connection: ExternalConnectionId,
        calendars: Vec<DiscoveredCalendar>,
    ) -> Result<Vec<ConnectedCalendar>, ConnectedCalendarStorageError>;
    async fn list_for_connection(
        &self,
        owner: UserId,
        connection: ExternalConnectionId,
    ) -> Result<Vec<ConnectedCalendar>, ConnectedCalendarStorageError>;
    /// Resolve a selected calendar by its internal EME identifier.
    /// Only calendars backed by an active provider connection are returned.
    async fn get_selected(
        &self,
        id: ConnectedCalendarId,
    ) -> Result<Option<ConnectedCalendar>, ConnectedCalendarStorageError>;
    async fn update(
        &self,
        owner: UserId,
        id: ConnectedCalendarId,
        selected: Option<bool>,
        access_mode: Option<CalendarAccessMode>,
    ) -> Result<ConnectedCalendar, ConnectedCalendarStorageError>;
    async fn share_with_family(
        &self,
        owner: UserId,
        id: ConnectedCalendarId,
        family_id: FamilyId,
        access: CalendarFamilyAccess,
    ) -> Result<ConnectedCalendar, ConnectedCalendarStorageError>;
    async fn unshare_from_family(
        &self,
        owner: UserId,
        id: ConnectedCalendarId,
    ) -> Result<ConnectedCalendar, ConnectedCalendarStorageError>;
}

#[derive(Clone)]
pub struct SqliteConnectedCalendarStore {
    db: std::sync::Arc<std::sync::Mutex<Connection>>,
}

impl SqliteConnectedCalendarStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, rusqlite::Error> {
        let db = Connection::open(path)?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        db.execute_batch("PRAGMA foreign_keys=ON;")?;
        db.execute_batch(include_str!("../migrations/0001_identity_and_families.sql"))?;
        db.execute_batch(include_str!("../migrations/0002_family_invitations.sql"))?;
        db.execute_batch(include_str!("../migrations/0003_external_connections.sql"))?;
        db.execute_batch(include_str!("../migrations/0004_connected_calendars.sql"))?;
        let has_family_share_columns: bool = db.query_row(
            "SELECT COUNT(*) = 2 FROM pragma_table_info('connected_calendars')
             WHERE name IN ('family_id', 'family_access')",
            [],
            |row| row.get(0),
        )?;
        if !has_family_share_columns {
            db.execute_batch(include_str!(
                "../migrations/0005_calendar_family_sharing.sql"
            ))?;
        }
        Ok(Self {
            db: std::sync::Arc::new(std::sync::Mutex::new(db)),
        })
    }
}

fn backend(error: rusqlite::Error) -> ConnectedCalendarStorageError {
    ConnectedCalendarStorageError::Backend(error.to_string())
}

type CalendarRow = (
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    bool,
    String,
    Option<String>,
    Option<String>,
);

fn load_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CalendarRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get::<_, i64>(7)? != 0,
        row.get(8)?,
        row.get(9)?,
        row.get(10)?,
    ))
}

fn parse_calendar(row: CalendarRow) -> Result<ConnectedCalendar, ConnectedCalendarStorageError> {
    let (
        id,
        owner,
        connection,
        provider_id,
        name,
        zone,
        role,
        selected,
        access,
        family,
        family_access,
    ) = row;
    Ok(ConnectedCalendar {
        id: ConnectedCalendarId::from_str(&id)
            .map_err(|e| ConnectedCalendarStorageError::Backend(e.to_string()))?,
        owner_id: UserId::from_str(&owner)
            .map_err(|e| ConnectedCalendarStorageError::Backend(e.to_string()))?,
        connection_id: ExternalConnectionId::from_str(&connection)
            .map_err(|e| ConnectedCalendarStorageError::Backend(e.to_string()))?,
        provider_calendar_id: provider_id,
        display_name: name,
        time_zone: zone,
        provider_access_role: role,
        selected,
        access_mode: CalendarAccessMode::parse(&access)?,
        family_id: family
            .map(|value| {
                FamilyId::from_str(&value)
                    .map_err(|error| ConnectedCalendarStorageError::Backend(error.to_string()))
            })
            .transpose()?,
        family_access: family_access
            .map(|value| CalendarFamilyAccess::parse(&value))
            .transpose()?,
    })
}

const SELECT_COLUMNS: &str = "id,owner_id,connection_id,provider_calendar_id,display_name,time_zone,provider_access_role,selected,access_mode,family_id,family_access";

#[async_trait]
impl ConnectedCalendarStore for SqliteConnectedCalendarStore {
    async fn sync_discovered(
        &self,
        owner: UserId,
        connection: ExternalConnectionId,
        calendars: Vec<DiscoveredCalendar>,
    ) -> Result<Vec<ConnectedCalendar>, ConnectedCalendarStorageError> {
        let mut db = self.db.lock().map_err(|_| {
            ConnectedCalendarStorageError::Backend("calendar database lock poisoned".into())
        })?;
        let tx = db.transaction().map_err(backend)?;
        let active: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM external_connections WHERE id=?1 AND owner_id=?2 AND provider='google' AND status='active')",
            params![connection.to_string(), owner.to_string()], |row| row.get(0),
        ).map_err(backend)?;
        if !active {
            return Err(ConnectedCalendarStorageError::ConnectionNotFound);
        }
        let now = chrono::Utc::now().to_rfc3339();
        for calendar in calendars {
            if calendar.provider_calendar_id.trim().is_empty() {
                return Err(ConnectedCalendarStorageError::EmptyProviderCalendarId);
            }
            tx.execute(
                "INSERT INTO connected_calendars(
                    id,owner_id,connection_id,provider_calendar_id,display_name,time_zone,
                    provider_access_role,selected,access_mode,created_at,updated_at
                 ) VALUES(?1,?2,?3,?4,?5,?6,?7,0,'read_only',?8,?8)
                 ON CONFLICT(connection_id,provider_calendar_id) DO UPDATE SET
                    display_name=excluded.display_name,
                    time_zone=excluded.time_zone,
                    provider_access_role=excluded.provider_access_role,
                    updated_at=excluded.updated_at",
                params![
                    ConnectedCalendarId::new().to_string(),
                    owner.to_string(),
                    connection.to_string(),
                    calendar.provider_calendar_id,
                    calendar.display_name,
                    calendar.time_zone,
                    calendar.provider_access_role,
                    now
                ],
            )
            .map_err(backend)?;
        }
        tx.commit().map_err(backend)?;
        let sql = format!(
            "SELECT {SELECT_COLUMNS} FROM connected_calendars WHERE owner_id=?1 AND connection_id=?2 ORDER BY display_name,provider_calendar_id"
        );
        let mut statement = db.prepare(&sql).map_err(backend)?;
        let rows = statement
            .query_map(params![owner.to_string(), connection.to_string()], load_row)
            .map_err(backend)?;
        rows.map(|row| parse_calendar(row.map_err(backend)?))
            .collect()
    }

    async fn list_for_connection(
        &self,
        owner: UserId,
        connection: ExternalConnectionId,
    ) -> Result<Vec<ConnectedCalendar>, ConnectedCalendarStorageError> {
        let db = self.db.lock().map_err(|_| {
            ConnectedCalendarStorageError::Backend("calendar database lock poisoned".into())
        })?;
        let active: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM external_connections WHERE id=?1 AND owner_id=?2 AND provider='google' AND status='active')",
            params![connection.to_string(), owner.to_string()], |row| row.get(0),
        ).map_err(backend)?;
        if !active {
            return Err(ConnectedCalendarStorageError::ConnectionNotFound);
        }
        let sql = format!(
            "SELECT {SELECT_COLUMNS} FROM connected_calendars WHERE owner_id=?1 AND connection_id=?2 ORDER BY display_name,provider_calendar_id"
        );
        let mut statement = db.prepare(&sql).map_err(backend)?;
        let rows = statement
            .query_map(params![owner.to_string(), connection.to_string()], load_row)
            .map_err(backend)?;
        rows.map(|row| parse_calendar(row.map_err(backend)?))
            .collect()
    }

    async fn get_selected(
        &self,
        id: ConnectedCalendarId,
    ) -> Result<Option<ConnectedCalendar>, ConnectedCalendarStorageError> {
        let db = self.db.lock().map_err(|_| {
            ConnectedCalendarStorageError::Backend("calendar database lock poisoned".into())
        })?;
        let sql = format!(
            "SELECT {SELECT_COLUMNS} FROM connected_calendars cal
             WHERE cal.id=?1 AND cal.selected=1 AND EXISTS(
                SELECT 1 FROM external_connections c
                WHERE c.id=cal.connection_id AND c.owner_id=cal.owner_id
                  AND c.provider='google' AND c.status='active')"
        );
        let row = db
            .query_row(&sql, [id.to_string()], load_row)
            .optional()
            .map_err(backend)?;
        row.map(parse_calendar).transpose()
    }

    async fn update(
        &self,
        owner: UserId,
        id: ConnectedCalendarId,
        selected: Option<bool>,
        access_mode: Option<CalendarAccessMode>,
    ) -> Result<ConnectedCalendar, ConnectedCalendarStorageError> {
        let db = self.db.lock().map_err(|_| {
            ConnectedCalendarStorageError::Backend("calendar database lock poisoned".into())
        })?;
        if access_mode == Some(CalendarAccessMode::ReadWrite) {
            let scopes: Option<String> = db
                .query_row(
                    "SELECT c.granted_scopes FROM connected_calendars cal
                 JOIN external_connections c ON c.id=cal.connection_id
                 WHERE cal.id=?1 AND cal.owner_id=?2 AND c.status='active'",
                    params![id.to_string(), owner.to_string()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(backend)?;
            let scopes = scopes.ok_or(ConnectedCalendarStorageError::CalendarNotFound)?;
            let scopes: Vec<String> = serde_json::from_str(&scopes)
                .map_err(|error| ConnectedCalendarStorageError::Backend(error.to_string()))?;
            if !scopes.iter().any(|scope| {
                scope == "https://www.googleapis.com/auth/calendar.events"
                    || scope == "https://www.googleapis.com/auth/calendar"
            }) {
                return Err(ConnectedCalendarStorageError::InsufficientScopes);
            }
        }
        let selected_value = selected.map(|value| if value { 1_i64 } else { 0_i64 });
        let access_value = access_mode.map(CalendarAccessMode::as_str);
        let sql = format!(
            "UPDATE connected_calendars SET selected=COALESCE(?1,selected),access_mode=COALESCE(?2,access_mode),family_id=CASE WHEN ?1=0 THEN NULL ELSE family_id END,family_access=CASE WHEN ?1=0 THEN NULL WHEN ?2='read_only' AND family_access='contributor' THEN 'viewer' ELSE family_access END,updated_at=?3
             WHERE id=?4 AND owner_id=?5 AND EXISTS(
                SELECT 1 FROM external_connections c WHERE c.id=connected_calendars.connection_id
                AND c.owner_id=connected_calendars.owner_id AND c.status='active')
             RETURNING {SELECT_COLUMNS}"
        );
        let row = db
            .query_row(
                &sql,
                params![
                    selected_value,
                    access_value,
                    chrono::Utc::now().to_rfc3339(),
                    id.to_string(),
                    owner.to_string()
                ],
                load_row,
            )
            .optional()
            .map_err(backend)?;
        row.map(parse_calendar)
            .transpose()?
            .ok_or(ConnectedCalendarStorageError::CalendarNotFound)
    }
    async fn share_with_family(
        &self,
        owner: UserId,
        id: ConnectedCalendarId,
        family_id: FamilyId,
        access: CalendarFamilyAccess,
    ) -> Result<ConnectedCalendar, ConnectedCalendarStorageError> {
        let db = self.db.lock().map_err(|_| {
            ConnectedCalendarStorageError::Backend("calendar database lock poisoned".into())
        })?;
        let row = db
            .query_row(
                "SELECT selected,access_mode FROM connected_calendars cal
             JOIN external_connections c ON c.id=cal.connection_id
             WHERE cal.id=?1 AND cal.owner_id=?2 AND c.status='active'",
                params![id.to_string(), owner.to_string()],
                |row| Ok((row.get::<_, i64>(0)? != 0, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(backend)?
            .ok_or(ConnectedCalendarStorageError::CalendarNotFound)?;
        if !row.0 {
            return Err(ConnectedCalendarStorageError::FamilyShareRequiresSelected);
        }
        if access == CalendarFamilyAccess::Contributor && row.1 != "read_write" {
            return Err(ConnectedCalendarStorageError::FamilyWriteRequiresOwnerWrite);
        }
        let sql = format!(
            "UPDATE connected_calendars SET family_id=?1,family_access=?2,updated_at=?3
             WHERE id=?4 AND owner_id=?5 AND selected=1 AND EXISTS(
                SELECT 1 FROM external_connections c WHERE c.id=connected_calendars.connection_id
                AND c.owner_id=connected_calendars.owner_id AND c.status='active')
             RETURNING {SELECT_COLUMNS}"
        );
        let row = db
            .query_row(
                &sql,
                params![
                    family_id.to_string(),
                    access.as_str(),
                    chrono::Utc::now().to_rfc3339(),
                    id.to_string(),
                    owner.to_string()
                ],
                load_row,
            )
            .optional()
            .map_err(backend)?;
        row.map(parse_calendar)
            .transpose()?
            .ok_or(ConnectedCalendarStorageError::CalendarNotFound)
    }

    async fn unshare_from_family(
        &self,
        owner: UserId,
        id: ConnectedCalendarId,
    ) -> Result<ConnectedCalendar, ConnectedCalendarStorageError> {
        let db = self.db.lock().map_err(|_| {
            ConnectedCalendarStorageError::Backend("calendar database lock poisoned".into())
        })?;
        let sql = format!(
            "UPDATE connected_calendars SET family_id=NULL,family_access=NULL,updated_at=?1
             WHERE id=?2 AND owner_id=?3 AND EXISTS(
                SELECT 1 FROM external_connections c WHERE c.id=connected_calendars.connection_id
                AND c.owner_id=connected_calendars.owner_id AND c.status='active')
             RETURNING {SELECT_COLUMNS}"
        );
        let row = db
            .query_row(
                &sql,
                params![
                    chrono::Utc::now().to_rfc3339(),
                    id.to_string(),
                    owner.to_string()
                ],
                load_row,
            )
            .optional()
            .map_err(backend)?;
        row.map(parse_calendar)
            .transpose()?
            .ok_or(ConnectedCalendarStorageError::CalendarNotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ExternalConnectionStore, NewExternalConnection, SqliteExternalConnectionStore};

    fn path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("eme-calendars-{}.sqlite", Uuid::new_v4()))
    }

    #[test]
    fn family_sharing_migration_is_idempotent() {
        let path = path();
        drop(SqliteConnectedCalendarStore::open(&path).unwrap());
        drop(SqliteConnectedCalendarStore::open(&path).unwrap());
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn sync_preserves_user_selection_and_isolated_by_owner() {
        let path = path();
        let owner = UserId::new();
        let other = UserId::new();
        let connections = SqliteExternalConnectionStore::open(&path, [0x42; 32]).unwrap();
        let connection = connections
            .create_connection(NewExternalConnection {
                owner_id: owner,
                provider: "google".into(),
                provider_subject: "subject".into(),
                display_email: None,
                refresh_token: "secret".into(),
                granted_scopes: vec!["https://www.googleapis.com/auth/calendar.events".into()],
            })
            .await
            .unwrap();
        let calendars = SqliteConnectedCalendarStore::open(&path).unwrap();
        let discovered = DiscoveredCalendar {
            provider_calendar_id: "primary".into(),
            display_name: Some("Primary".into()),
            time_zone: Some("Europe/Madrid".into()),
            provider_access_role: Some("owner".into()),
        };
        let first = calendars
            .sync_discovered(owner, connection.id, vec![discovered.clone()])
            .await
            .unwrap();
        assert_eq!(first.len(), 1);
        assert!(!first[0].selected);
        assert!(calendars.get_selected(first[0].id).await.unwrap().is_none());
        assert!(matches!(
            calendars
                .share_with_family(
                    owner,
                    first[0].id,
                    FamilyId::new(),
                    CalendarFamilyAccess::Viewer
                )
                .await,
            Err(ConnectedCalendarStorageError::FamilyShareRequiresSelected)
        ));
        let selected = calendars
            .update(
                owner,
                first[0].id,
                Some(true),
                Some(CalendarAccessMode::ReadWrite),
            )
            .await
            .unwrap();
        assert!(selected.selected);
        let accessible = calendars.get_selected(selected.id).await.unwrap().unwrap();
        assert_eq!(accessible.provider_calendar_id, "primary");
        assert_eq!(accessible.owner_id, owner);
        let refreshed = calendars
            .sync_discovered(owner, connection.id, vec![discovered])
            .await
            .unwrap();
        assert!(refreshed[0].selected);
        assert_eq!(refreshed[0].access_mode, CalendarAccessMode::ReadWrite);
        let family_id = FamilyId::new();
        let shared = calendars
            .share_with_family(
                owner,
                first[0].id,
                family_id,
                CalendarFamilyAccess::Contributor,
            )
            .await
            .unwrap();
        assert_eq!(shared.family_id, Some(family_id));
        assert_eq!(
            shared.family_access,
            Some(CalendarFamilyAccess::Contributor)
        );
        let downgraded = calendars
            .update(owner, first[0].id, None, Some(CalendarAccessMode::ReadOnly))
            .await
            .unwrap();
        assert_eq!(downgraded.family_access, Some(CalendarFamilyAccess::Viewer));
        let unshared = calendars
            .unshare_from_family(owner, first[0].id)
            .await
            .unwrap();
        assert_eq!(unshared.family_id, None);
        assert_eq!(unshared.family_access, None);
        assert!(matches!(
            calendars.list_for_connection(other, connection.id).await,
            Err(ConnectedCalendarStorageError::ConnectionNotFound)
        ));
        drop(calendars);
        drop(connections);
        let _ = std::fs::remove_file(path);
    }
}
