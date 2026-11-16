//! SQLite-backed family, membership and invitation storage.
//!
//! Family and invitation operations share one connection so invitation
//! acceptance can create the membership and consume the token in one SQLite
//! transaction. Connections are serialized through a Tokio mutex; operations
//! are intentionally small and synchronous once the lock has been acquired.
use std::{path::Path, str::FromStr, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    Family, FamilyId, FamilyInvitationStore, FamilyInvitationSummary, FamilyMember,
    FamilyRelationship, FamilyRole, FamilyStorageError, FamilyStore, InvitationStorageError,
    InvitationTargetIdentity, InvitationToken, UserId,
};

#[derive(Clone)]
pub struct SqliteFamilyStore {
    connection: Arc<Mutex<Connection>>,
}

impl SqliteFamilyStore {
    /// Open (or create) a database and apply all family/identity migrations.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, rusqlite::Error> {
        let mut connection = Connection::open(path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA foreign_keys = ON;")?;
        connection.execute_batch(include_str!("../migrations/0001_identity_and_families.sql"))?;
        connection.execute_batch(include_str!("../migrations/0002_family_invitations.sql"))?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    /// Ensure the internal user row exists before using family storage.
    /// Identity metadata remains owned by UserIdentityStore; this row only
    /// satisfies the relational foreign keys used by family records.
    pub async fn ensure_user(&self, user_id: UserId) -> Result<(), FamilyStorageError> {
        let connection = self.connection.lock().await;
        ensure_user(&connection, user_id).map_err(family_backend)
    }
}

fn family_backend(error: rusqlite::Error) -> FamilyStorageError {
    FamilyStorageError::Backend(error.to_string())
}

fn invitation_backend(error: rusqlite::Error) -> InvitationStorageError {
    InvitationStorageError::Backend(error.to_string())
}

fn ensure_user(connection: &Connection, user_id: UserId) -> Result<(), rusqlite::Error> {
    let now = Utc::now().to_rfc3339();
    connection.execute(
        "INSERT OR IGNORE INTO users(id, display_name, created_at, updated_at) VALUES (?1, NULL, ?2, ?2)",
        params![user_id.to_string(), now],
    )?;
    Ok(())
}

fn ensure_user_tx(tx: &Transaction<'_>, user_id: UserId) -> Result<(), rusqlite::Error> {
    let now = Utc::now().to_rfc3339();
    tx.execute(
        "INSERT OR IGNORE INTO users(id, display_name, created_at, updated_at) VALUES (?1, NULL, ?2, ?2)",
        params![user_id.to_string(), now],
    )?;
    Ok(())
}

fn role_text(role: FamilyRole) -> &'static str {
    match role {
        FamilyRole::Owner => "owner",
        FamilyRole::Admin => "admin",
        FamilyRole::Member => "member",
    }
}
fn parse_role(value: &str) -> Result<FamilyRole, String> {
    match value {
        "owner" => Ok(FamilyRole::Owner),
        "admin" => Ok(FamilyRole::Admin),
        "member" => Ok(FamilyRole::Member),
        _ => Err(format!("invalid family role in database: {value}")),
    }
}
fn relationship_text(value: FamilyRelationship) -> &'static str {
    match value {
        FamilyRelationship::Father => "father",
        FamilyRelationship::Mother => "mother",
        FamilyRelationship::Son => "son",
        FamilyRelationship::Daughter => "daughter",
        FamilyRelationship::Grandfather => "grandfather",
        FamilyRelationship::Grandmother => "grandmother",
        FamilyRelationship::Partner => "partner",
        FamilyRelationship::Other => "other",
    }
}
fn parse_relationship(value: &str) -> Result<FamilyRelationship, String> {
    match value {
        "father" => Ok(FamilyRelationship::Father),
        "mother" => Ok(FamilyRelationship::Mother),
        "son" => Ok(FamilyRelationship::Son),
        "daughter" => Ok(FamilyRelationship::Daughter),
        "grandfather" => Ok(FamilyRelationship::Grandfather),
        "grandmother" => Ok(FamilyRelationship::Grandmother),
        "partner" => Ok(FamilyRelationship::Partner),
        "other" => Ok(FamilyRelationship::Other),
        _ => Err(format!("invalid family relationship in database: {value}")),
    }
}
fn parse_time(value: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.with_timezone(&Utc))
        .map_err(|error| error.to_string())
}

fn load_family(
    connection: &Connection,
    family_id: FamilyId,
) -> Result<Option<Family>, rusqlite::Error> {
    let row = connection
        .query_row(
            "SELECT name, creator_id, created_at, updated_at FROM families WHERE id = ?1",
            [family_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()?;
    row.map(|(name, creator, created, updated)| {
        Ok(Family {
            id: family_id,
            name,
            creator_id: UserId::from_str(&creator).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    1,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?,
            created_at: parse_time(&created).map_err(|e| {
                rusqlite::Error::InvalidColumnType(2, e, rusqlite::types::Type::Text)
            })?,
            updated_at: parse_time(&updated).map_err(|e| {
                rusqlite::Error::InvalidColumnType(3, e, rusqlite::types::Type::Text)
            })?,
        })
    })
    .transpose()
}

fn load_member(
    connection: &Connection,
    family_id: FamilyId,
    user_id: UserId,
) -> Result<Option<FamilyMember>, rusqlite::Error> {
    let row = connection
        .query_row(
            "SELECT role, relationship, joined_at FROM family_members WHERE family_id = ?1 AND user_id = ?2",
            params![family_id.to_string(), user_id.to_string()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)),
        )
        .optional()?;
    row.map(|(role, relationship, joined)| {
        Ok(FamilyMember {
            family_id,
            user_id,
            role: parse_role(&role).map_err(|e| {
                rusqlite::Error::InvalidColumnType(0, e, rusqlite::types::Type::Text)
            })?,
            relationship: parse_relationship(&relationship).map_err(|e| {
                rusqlite::Error::InvalidColumnType(1, e, rusqlite::types::Type::Text)
            })?,
            joined_at: parse_time(&joined).map_err(|e| {
                rusqlite::Error::InvalidColumnType(2, e, rusqlite::types::Type::Text)
            })?,
        })
    })
    .transpose()
}

fn load_members(
    connection: &Connection,
    family_id: FamilyId,
) -> Result<Vec<FamilyMember>, rusqlite::Error> {
    let mut statement = connection.prepare(
        "SELECT user_id, role, relationship, joined_at FROM family_members WHERE family_id = ?1 ORDER BY joined_at, user_id",
    )?;
    let rows = statement.query_map([family_id.to_string()], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    let mut members = Vec::new();
    for row in rows {
        let (user, role, relationship, joined) = row?;
        let user_id = UserId::from_str(&user).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?;
        members.push(FamilyMember {
            family_id,
            user_id,
            role: parse_role(&role).map_err(|e| {
                rusqlite::Error::InvalidColumnType(1, e, rusqlite::types::Type::Text)
            })?,
            relationship: parse_relationship(&relationship).map_err(|e| {
                rusqlite::Error::InvalidColumnType(2, e, rusqlite::types::Type::Text)
            })?,
            joined_at: parse_time(&joined).map_err(|e| {
                rusqlite::Error::InvalidColumnType(3, e, rusqlite::types::Type::Text)
            })?,
        });
    }
    Ok(members)
}

#[async_trait]
impl FamilyStore for SqliteFamilyStore {
    async fn create_family(
        &self,
        family: Family,
        owner: FamilyMember,
    ) -> Result<(), FamilyStorageError> {
        if family.name.trim().is_empty() {
            return Err(FamilyStorageError::EmptyName);
        }
        if family.id != owner.family_id
            || family.creator_id != owner.user_id
            || owner.role != FamilyRole::Owner
        {
            return Err(FamilyStorageError::OwnerInvariant);
        }
        let mut connection = self.connection.lock().await;
        let tx = connection.transaction().map_err(family_backend)?;
        if load_family(&tx, family.id)
            .map_err(family_backend)?
            .is_some()
        {
            return Err(FamilyStorageError::FamilyAlreadyExists(family.id));
        }
        ensure_user_tx(&tx, family.creator_id).map_err(family_backend)?;
        tx.execute(
            "INSERT INTO families(id, name, creator_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![family.id.to_string(), family.name.trim(), family.creator_id.to_string(), family.created_at.to_rfc3339(), family.updated_at.to_rfc3339()],
        ).map_err(family_backend)?;
        tx.execute(
            "INSERT INTO family_members(family_id, user_id, role, relationship, joined_at) VALUES (?1, ?2, 'owner', ?3, ?4)",
            params![family.id.to_string(), owner.user_id.to_string(), relationship_text(owner.relationship), owner.joined_at.to_rfc3339()],
        ).map_err(family_backend)?;
        tx.commit().map_err(family_backend)
    }

    async fn list_families(&self, user_id: UserId) -> Result<Vec<Family>, FamilyStorageError> {
        let connection = self.connection.lock().await;
        let mut statement = connection.prepare(
            "SELECT f.id, f.name, f.creator_id, f.created_at, f.updated_at FROM families f JOIN family_members m ON m.family_id = f.id WHERE m.user_id = ?1 ORDER BY f.created_at, f.id",
        ).map_err(family_backend)?;
        let rows = statement
            .query_map([user_id.to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(family_backend)?;
        let mut result = Vec::new();
        for row in rows {
            let (id, name, creator, created, updated) = row.map_err(family_backend)?;
            let id =
                FamilyId::from_str(&id).map_err(|e| FamilyStorageError::Backend(e.to_string()))?;
            result.push(Family {
                id,
                name,
                creator_id: UserId::from_str(&creator)
                    .map_err(|e| FamilyStorageError::Backend(e.to_string()))?,
                created_at: parse_time(&created).map_err(FamilyStorageError::Backend)?,
                updated_at: parse_time(&updated).map_err(FamilyStorageError::Backend)?,
            });
        }
        Ok(result)
    }

    async fn get_family(&self, family_id: FamilyId) -> Result<Option<Family>, FamilyStorageError> {
        let connection = self.connection.lock().await;
        load_family(&connection, family_id).map_err(family_backend)
    }

    async fn rename_family(
        &self,
        family_id: FamilyId,
        name: &str,
    ) -> Result<Family, FamilyStorageError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(FamilyStorageError::EmptyName);
        }
        let connection = self.connection.lock().await;
        let changed = connection
            .execute(
                "UPDATE families SET name = ?1, updated_at = ?2 WHERE id = ?3",
                params![name, Utc::now().to_rfc3339(), family_id.to_string()],
            )
            .map_err(family_backend)?;
        if changed == 0 {
            return Err(FamilyStorageError::FamilyNotFound(family_id));
        }
        load_family(&connection, family_id)
            .map_err(family_backend)?
            .ok_or(FamilyStorageError::FamilyNotFound(family_id))
    }

    async fn delete_family(&self, family_id: FamilyId) -> Result<(), FamilyStorageError> {
        let connection = self.connection.lock().await;
        let changed = connection
            .execute(
                "DELETE FROM families WHERE id = ?1",
                [family_id.to_string()],
            )
            .map_err(family_backend)?;
        if changed == 0 {
            return Err(FamilyStorageError::FamilyNotFound(family_id));
        }
        Ok(())
    }

    async fn list_members(
        &self,
        family_id: FamilyId,
    ) -> Result<Vec<FamilyMember>, FamilyStorageError> {
        let connection = self.connection.lock().await;
        if load_family(&connection, family_id)
            .map_err(family_backend)?
            .is_none()
        {
            return Err(FamilyStorageError::FamilyNotFound(family_id));
        }
        load_members(&connection, family_id).map_err(family_backend)
    }

    async fn add_member(&self, member: FamilyMember) -> Result<(), FamilyStorageError> {
        if member.role == FamilyRole::Owner {
            return Err(FamilyStorageError::OwnerRoleRequiresTransfer);
        }
        let mut connection = self.connection.lock().await;
        let tx = connection.transaction().map_err(family_backend)?;
        if load_family(&tx, member.family_id)
            .map_err(family_backend)?
            .is_none()
        {
            return Err(FamilyStorageError::FamilyNotFound(member.family_id));
        }
        ensure_user_tx(&tx, member.user_id).map_err(family_backend)?;
        if load_member(&tx, member.family_id, member.user_id)
            .map_err(family_backend)?
            .is_some()
        {
            return Err(FamilyStorageError::MembershipAlreadyExists {
                family_id: member.family_id,
                user_id: member.user_id,
            });
        }
        tx.execute(
            "INSERT INTO family_members(family_id, user_id, role, relationship, joined_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![member.family_id.to_string(), member.user_id.to_string(), role_text(member.role), relationship_text(member.relationship), member.joined_at.to_rfc3339()],
        ).map_err(family_backend)?;
        tx.commit().map_err(family_backend)
    }

    async fn remove_member(
        &self,
        family_id: FamilyId,
        user_id: UserId,
    ) -> Result<(), FamilyStorageError> {
        self.remove_non_owner(family_id, user_id).await
    }
    async fn leave_family(
        &self,
        family_id: FamilyId,
        user_id: UserId,
    ) -> Result<(), FamilyStorageError> {
        self.remove_non_owner(family_id, user_id).await
    }

    async fn change_member_role(
        &self,
        family_id: FamilyId,
        user_id: UserId,
        role: FamilyRole,
    ) -> Result<FamilyMember, FamilyStorageError> {
        if role == FamilyRole::Owner {
            return Err(FamilyStorageError::OwnerRoleRequiresTransfer);
        }
        let connection = self.connection.lock().await;
        let Some(member) = load_member(&connection, family_id, user_id).map_err(family_backend)?
        else {
            return Err(FamilyStorageError::MembershipNotFound { family_id, user_id });
        };
        if member.role == FamilyRole::Owner {
            return Err(FamilyStorageError::OwnerRoleRequiresTransfer);
        }
        connection
            .execute(
                "UPDATE family_members SET role = ?1 WHERE family_id = ?2 AND user_id = ?3",
                params![role_text(role), family_id.to_string(), user_id.to_string()],
            )
            .map_err(family_backend)?;
        load_member(&connection, family_id, user_id)
            .map_err(family_backend)?
            .ok_or(FamilyStorageError::MembershipNotFound { family_id, user_id })
    }

    async fn change_member_relationship(
        &self,
        family_id: FamilyId,
        user_id: UserId,
        relationship: FamilyRelationship,
    ) -> Result<FamilyMember, FamilyStorageError> {
        let connection = self.connection.lock().await;
        let changed = connection
            .execute(
                "UPDATE family_members SET relationship = ?1 WHERE family_id = ?2 AND user_id = ?3",
                params![
                    relationship_text(relationship),
                    family_id.to_string(),
                    user_id.to_string()
                ],
            )
            .map_err(family_backend)?;
        if changed == 0 {
            return Err(FamilyStorageError::MembershipNotFound { family_id, user_id });
        }
        load_member(&connection, family_id, user_id)
            .map_err(family_backend)?
            .ok_or(FamilyStorageError::MembershipNotFound { family_id, user_id })
    }

    async fn transfer_ownership(
        &self,
        family_id: FamilyId,
        current_owner: UserId,
        new_owner: UserId,
    ) -> Result<(), FamilyStorageError> {
        let mut connection = self.connection.lock().await;
        let tx = connection.transaction().map_err(family_backend)?;
        if load_family(&tx, family_id)
            .map_err(family_backend)?
            .is_none()
        {
            return Err(FamilyStorageError::FamilyNotFound(family_id));
        }
        let current = load_member(&tx, family_id, current_owner)
            .map_err(family_backend)?
            .ok_or(FamilyStorageError::MembershipNotFound {
                family_id,
                user_id: current_owner,
            })?;
        if current.role != FamilyRole::Owner {
            return Err(FamilyStorageError::NotCurrentOwner);
        }
        if current_owner == new_owner {
            tx.commit().map_err(family_backend)?;
            return Ok(());
        }
        if load_member(&tx, family_id, new_owner)
            .map_err(family_backend)?
            .is_none()
        {
            return Err(FamilyStorageError::TransferTargetNotMember);
        }
        tx.execute(
            "UPDATE family_members SET role = 'admin' WHERE family_id = ?1 AND user_id = ?2",
            params![family_id.to_string(), current_owner.to_string()],
        )
        .map_err(family_backend)?;
        tx.execute(
            "UPDATE family_members SET role = 'owner' WHERE family_id = ?1 AND user_id = ?2",
            params![family_id.to_string(), new_owner.to_string()],
        )
        .map_err(family_backend)?;
        tx.commit().map_err(family_backend)
    }
}

impl SqliteFamilyStore {
    async fn remove_non_owner(
        &self,
        family_id: FamilyId,
        user_id: UserId,
    ) -> Result<(), FamilyStorageError> {
        let connection = self.connection.lock().await;
        let member = load_member(&connection, family_id, user_id)
            .map_err(family_backend)?
            .ok_or(FamilyStorageError::MembershipNotFound { family_id, user_id })?;
        if member.role == FamilyRole::Owner {
            return Err(FamilyStorageError::OwnerCannotLeave);
        }
        connection
            .execute(
                "DELETE FROM family_members WHERE family_id = ?1 AND user_id = ?2",
                params![family_id.to_string(), user_id.to_string()],
            )
            .map_err(family_backend)?;
        Ok(())
    }
}

#[async_trait]
impl FamilyInvitationStore for SqliteFamilyStore {
    async fn create_invitation(
        &self,
        family_id: FamilyId,
        inviter_id: UserId,
        target_identity: Option<InvitationTargetIdentity>,
        expires_at: DateTime<Utc>,
    ) -> Result<(FamilyInvitationSummary, InvitationToken), InvitationStorageError> {
        let now = Utc::now();
        if expires_at <= now {
            return Err(InvitationStorageError::InvalidExpiry);
        }
        let token = InvitationToken::generate();
        let id = Uuid::new_v4();
        let connection = self.connection.lock().await;
        let inviter_is_member: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM family_members WHERE family_id = ?1 AND user_id = ?2)",
                params![family_id.to_string(), inviter_id.to_string()],
                |row| row.get(0),
            )
            .map_err(invitation_backend)?;
        if !inviter_is_member {
            let exists: bool = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM families WHERE id = ?1)",
                    [family_id.to_string()],
                    |row| row.get(0),
                )
                .map_err(invitation_backend)?;
            return Err(if exists {
                InvitationStorageError::InviterNotMember
            } else {
                InvitationStorageError::FamilyNotFound(family_id)
            });
        }
        let (provider, subject) = target_identity
            .map(|identity| (Some(identity.provider), Some(identity.subject)))
            .unwrap_or((None, None));
        connection.execute(
            "INSERT INTO family_invitations(id, family_id, inviter_id, target_provider, target_subject, token_hash, created_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id.to_string(), family_id.to_string(), inviter_id.to_string(), provider, subject, token.hash(), now.to_rfc3339(), expires_at.to_rfc3339()],
        ).map_err(invitation_backend)?;
        Ok((
            FamilyInvitationSummary {
                id,
                family_id,
                inviter_id,
                target_identity: match (provider, subject) {
                    (Some(provider), Some(subject)) => {
                        Some(InvitationTargetIdentity { provider, subject })
                    }
                    _ => None,
                },
                created_at: now,
                expires_at,
                accepted_at: None,
                accepted_by: None,
                revoked_at: None,
            },
            token,
        ))
    }

    async fn list_invitations(
        &self,
        family_id: FamilyId,
    ) -> Result<Vec<FamilyInvitationSummary>, InvitationStorageError> {
        let connection = self.connection.lock().await;
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM families WHERE id = ?1)",
                [family_id.to_string()],
                |row| row.get(0),
            )
            .map_err(invitation_backend)?;
        if !exists {
            return Err(InvitationStorageError::FamilyNotFound(family_id));
        }
        let mut statement = connection.prepare(
            "SELECT id, inviter_id, target_provider, target_subject, created_at, expires_at, accepted_at, accepted_by, revoked_at FROM family_invitations WHERE family_id = ?1 ORDER BY created_at, id",
        ).map_err(invitation_backend)?;
        let rows = statement
            .query_map([family_id.to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                ))
            })
            .map_err(invitation_backend)?;
        let mut result = Vec::new();
        for row in rows {
            let (id, inviter, provider, subject, created, expires, accepted, accepted_by, revoked) =
                row.map_err(invitation_backend)?;
            result.push(FamilyInvitationSummary {
                id: Uuid::parse_str(&id)
                    .map_err(|e| InvitationStorageError::Backend(e.to_string()))?,
                family_id,
                inviter_id: UserId::from_str(&inviter)
                    .map_err(|e| InvitationStorageError::Backend(e.to_string()))?,
                target_identity: match (provider, subject) {
                    (Some(provider), Some(subject)) => {
                        Some(InvitationTargetIdentity { provider, subject })
                    }
                    (None, None) => None,
                    _ => {
                        return Err(InvitationStorageError::Backend(
                            "inconsistent target identity columns".into(),
                        ));
                    }
                },
                created_at: parse_time(&created).map_err(InvitationStorageError::Backend)?,
                expires_at: parse_time(&expires).map_err(InvitationStorageError::Backend)?,
                accepted_at: accepted
                    .as_deref()
                    .map(parse_time)
                    .transpose()
                    .map_err(InvitationStorageError::Backend)?,
                accepted_by: accepted_by
                    .as_deref()
                    .map(UserId::from_str)
                    .transpose()
                    .map_err(|e| InvitationStorageError::Backend(e.to_string()))?,
                revoked_at: revoked
                    .as_deref()
                    .map(parse_time)
                    .transpose()
                    .map_err(InvitationStorageError::Backend)?,
            });
        }
        Ok(result)
    }

    async fn revoke_invitation(
        &self,
        family_id: FamilyId,
        invitation_id: Uuid,
    ) -> Result<FamilyInvitationSummary, InvitationStorageError> {
        let connection = self.connection.lock().await;
        let accepted: Option<String> = connection
            .query_row(
                "SELECT accepted_at FROM family_invitations WHERE id = ?1 AND family_id = ?2",
                params![invitation_id.to_string(), family_id.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(invitation_backend)?
            .ok_or(InvitationStorageError::NotFound)?;
        if accepted.is_some() {
            return Err(InvitationStorageError::AlreadyAccepted);
        }
        connection.execute(
            "UPDATE family_invitations SET revoked_at = COALESCE(revoked_at, ?1) WHERE id = ?2 AND family_id = ?3",
            params![Utc::now().to_rfc3339(), invitation_id.to_string(), family_id.to_string()],
        ).map_err(invitation_backend)?;
        drop(connection);
        self.list_invitations(family_id)
            .await?
            .into_iter()
            .find(|item| item.id == invitation_id)
            .ok_or(InvitationStorageError::NotFound)
    }

    async fn accept_invitation(
        &self,
        token: &InvitationToken,
        accepting_identity: &InvitationTargetIdentity,
        user_id: UserId,
        relationship: FamilyRelationship,
    ) -> Result<FamilyMember, InvitationStorageError> {
        let mut connection = self.connection.lock().await;
        let tx = connection.transaction().map_err(invitation_backend)?;
        let row = tx.query_row(
            "SELECT id, family_id, target_provider, target_subject, expires_at, accepted_at, accepted_by, revoked_at FROM family_invitations WHERE token_hash = ?1",
            [token.hash()],
            |row| Ok((
                row.get::<_, String>(0)?, row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?, row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?, row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?, row.get::<_, Option<String>>(7)?,
            )),
        ).optional().map_err(invitation_backend)?.ok_or(InvitationStorageError::NotFound)?;
        let (id, family, provider, subject, expires, accepted_at, accepted_by, revoked_at) = row;
        if let (Some(provider), Some(subject)) = (provider, subject) {
            if provider != accepting_identity.provider || subject != accepting_identity.subject {
                return Err(InvitationStorageError::IdentityMismatch);
            }
        }
        let family_id = FamilyId::from_str(&family)
            .map_err(|e| InvitationStorageError::Backend(e.to_string()))?;
        if let Some(accepted_by) = accepted_by {
            let accepted_by = UserId::from_str(&accepted_by)
                .map_err(|e| InvitationStorageError::Backend(e.to_string()))?;
            if accepted_by != user_id {
                return Err(InvitationStorageError::AlreadyAccepted);
            }
            let member = load_member(&tx, family_id, user_id)
                .map_err(invitation_backend)?
                .ok_or(InvitationStorageError::AlreadyAccepted)?;
            tx.commit().map_err(invitation_backend)?;
            return Ok(member);
        }
        if accepted_at.is_some() {
            return Err(InvitationStorageError::AlreadyAccepted);
        }
        if revoked_at.is_some() {
            return Err(InvitationStorageError::Revoked);
        }
        if parse_time(&expires).map_err(InvitationStorageError::Backend)? <= Utc::now() {
            return Err(InvitationStorageError::Expired);
        }

        ensure_user_tx(&tx, user_id).map_err(invitation_backend)?;
        let member = FamilyMember {
            family_id,
            user_id,
            role: FamilyRole::Member,
            relationship,
            joined_at: Utc::now(),
        };
        tx.execute(
            "INSERT INTO family_members(family_id, user_id, role, relationship, joined_at) VALUES (?1, ?2, 'member', ?3, ?4)",
            params![family_id.to_string(), user_id.to_string(), relationship_text(relationship), member.joined_at.to_rfc3339()],
        ).map_err(|error| {
            if matches!(error, rusqlite::Error::SqliteFailure(ref code, _) if code.code == rusqlite::ErrorCode::ConstraintViolation) {
                InvitationStorageError::MembershipAlreadyExists
            } else { invitation_backend(error) }
        })?;
        tx.execute(
            "UPDATE family_invitations SET accepted_at = ?1, accepted_by = ?2 WHERE id = ?3 AND accepted_at IS NULL AND revoked_at IS NULL",
            params![Utc::now().to_rfc3339(), user_id.to_string(), id],
        ).map_err(invitation_backend)?;
        tx.commit().map_err(invitation_backend)?;
        Ok(member)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn database_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("eme-family-{}.sqlite", Uuid::new_v4()))
    }

    fn family(owner: UserId) -> (Family, FamilyMember) {
        let id = FamilyId::new();
        let now = Utc::now();
        (
            Family {
                id,
                name: "Home".into(),
                creator_id: owner,
                created_at: now,
                updated_at: now,
            },
            FamilyMember {
                family_id: id,
                user_id: owner,
                role: FamilyRole::Owner,
                relationship: FamilyRelationship::Other,
                joined_at: now,
            },
        )
    }

    fn identity(subject: &str) -> InvitationTargetIdentity {
        InvitationTargetIdentity::new("telegram", subject).unwrap()
    }

    #[tokio::test]
    async fn persists_families_members_and_invitations_across_reopen() {
        let path = database_path();
        let owner = UserId::new();
        let (family, membership) = family(owner);
        let invitation_id;
        {
            let store = SqliteFamilyStore::open(&path).unwrap();
            store
                .create_family(family.clone(), membership)
                .await
                .unwrap();
            let (invitation, _) = store
                .create_invitation(
                    family.id,
                    owner,
                    Some(identity("telegram-user")),
                    Utc::now() + Duration::days(1),
                )
                .await
                .unwrap();
            invitation_id = invitation.id;
        }
        {
            let store = SqliteFamilyStore::open(&path).unwrap();
            assert_eq!(
                store.get_family(family.id).await.unwrap(),
                Some(family.clone())
            );
            assert_eq!(store.list_members(family.id).await.unwrap().len(), 1);
            let invitations = store.list_invitations(family.id).await.unwrap();
            assert_eq!(invitations.len(), 1);
            assert_eq!(invitations[0].id, invitation_id);
            assert_eq!(
                invitations[0].target_identity,
                Some(identity("telegram-user"))
            );
        }
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn acceptance_is_transactional_single_use_and_idempotent() {
        let path = database_path();
        let store = SqliteFamilyStore::open(&path).unwrap();
        let owner = UserId::new();
        let (family, membership) = family(owner);
        store
            .create_family(family.clone(), membership)
            .await
            .unwrap();
        let (_, token) = store
            .create_invitation(
                family.id,
                owner,
                Some(identity("telegram-user")),
                Utc::now() + Duration::days(1),
            )
            .await
            .unwrap();
        let guest = UserId::new();
        let accepted = store
            .accept_invitation(
                &token,
                &identity("telegram-user"),
                guest,
                FamilyRelationship::Son,
            )
            .await
            .unwrap();
        assert_eq!(accepted.user_id, guest);
        let retry = store
            .accept_invitation(
                &token,
                &identity("telegram-user"),
                guest,
                FamilyRelationship::Other,
            )
            .await
            .unwrap();
        assert_eq!(retry, accepted);
        assert!(matches!(
            store
                .accept_invitation(
                    &token,
                    &identity("telegram-user"),
                    UserId::new(),
                    FamilyRelationship::Son
                )
                .await,
            Err(InvitationStorageError::AlreadyAccepted)
        ));
        assert_eq!(store.list_members(family.id).await.unwrap().len(), 2);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn ownership_transfer_and_member_removal_preserve_invariants() {
        let path = database_path();
        let store = SqliteFamilyStore::open(&path).unwrap();
        let owner = UserId::new();
        let guest = UserId::new();
        let (family, membership) = family(owner);
        store
            .create_family(family.clone(), membership)
            .await
            .unwrap();
        store
            .add_member(FamilyMember {
                family_id: family.id,
                user_id: guest,
                role: FamilyRole::Admin,
                relationship: FamilyRelationship::Partner,
                joined_at: Utc::now(),
            })
            .await
            .unwrap();
        store
            .transfer_ownership(family.id, owner, guest)
            .await
            .unwrap();
        let members = store.list_members(family.id).await.unwrap();
        assert_eq!(
            members
                .iter()
                .filter(|m| m.role == FamilyRole::Owner)
                .count(),
            1
        );
        store.remove_member(family.id, owner).await.unwrap();
        assert_eq!(store.list_members(family.id).await.unwrap().len(), 1);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn rejects_expired_revoked_and_identity_mismatched_invitations() {
        let path = database_path();
        let store = SqliteFamilyStore::open(&path).unwrap();
        let owner = UserId::new();
        let (family, membership) = family(owner);
        store
            .create_family(family.clone(), membership)
            .await
            .unwrap();
        let (wrong_identity, token) = store
            .create_invitation(
                family.id,
                owner,
                Some(identity("allowed")),
                Utc::now() + Duration::days(1),
            )
            .await
            .unwrap();
        assert!(matches!(
            store.validate_invitation(&token, &identity("other")).await,
            Err(InvitationStorageError::IdentityMismatch)
        ));
        store
            .revoke_invitation(family.id, wrong_identity.id)
            .await
            .unwrap();
        assert!(matches!(
            store
                .accept_invitation(&token, &identity("allowed"), UserId::new(), FamilyRelationship::Other)
                .await,
            Err(InvitationStorageError::Revoked)
        ));
        let (_, expired) = store
            .create_invitation(family.id, owner, None, Utc::now() + Duration::days(1))
            .await
            .unwrap();
        {
            let connection = store.connection.lock().await;
            connection
                .execute(
                    "UPDATE family_invitations SET expires_at = ?1 WHERE token_hash = ?2",
                    params![
                        (Utc::now() - Duration::seconds(1)).to_rfc3339(),
                        expired.hash()
                    ],
                )
                .unwrap();
        }
        assert!(matches!(
            store
                .accept_invitation(&expired, &identity("telegram"), UserId::new(), FamilyRelationship::Other)
                .await,
            Err(InvitationStorageError::Expired)
        ));
        let _ = std::fs::remove_file(path);
    }
}
