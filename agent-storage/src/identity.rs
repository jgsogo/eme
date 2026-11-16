//! Persistent EME user identities, independent from external login providers.
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    str::FromStr,
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserId(Uuid);

impl UserId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}
impl Default for UserId {
    fn default() -> Self {
        Self::new()
    }
}
impl std::fmt::Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl FromStr for UserId {
    type Err = uuid::Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(value).map(Self)
    }
}

/// Stable internal EME identity. Provider-specific identifiers belong in UserIdentity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// An external identity linked to an EME user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserIdentity {
    pub provider: String,
    pub subject: String,
    pub user_id: UserId,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Error)]
pub enum IdentityStoreError {
    #[error("identity storage I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("identity storage serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("user {0} was not found")]
    UserNotFound(UserId),
    #[error("identity ({provider}, {subject}) is already linked to another user")]
    IdentityAlreadyLinked { provider: String, subject: String },
    #[error("identity provider and subject must not be empty")]
    EmptyIdentity,
}

/// Atomic operations for resolving and linking external identities.
/// Implementations must enforce uniqueness of (provider, subject).
pub trait UserIdentityStore: Send + Sync {
    fn resolve_or_create(
        &self,
        provider: &str,
        subject: &str,
        display_name: Option<&str>,
    ) -> Result<(User, UserIdentity), IdentityStoreError>;
    fn get_user(&self, user_id: UserId) -> Result<Option<User>, IdentityStoreError>;
    fn find_identity(
        &self,
        provider: &str,
        subject: &str,
    ) -> Result<Option<UserIdentity>, IdentityStoreError>;
    fn link_identity(
        &self,
        user_id: UserId,
        provider: &str,
        subject: &str,
    ) -> Result<UserIdentity, IdentityStoreError>;
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct IdentityState {
    users: HashMap<UserId, User>,
    identities: HashMap<String, UserIdentity>,
}

fn identity_key(provider: &str, subject: &str) -> String {
    format!("{provider}\\0{subject}")
}

impl IdentityState {
    fn resolve_or_create(
        &mut self,
        provider: &str,
        subject: &str,
        display_name: Option<&str>,
    ) -> Result<(User, UserIdentity), IdentityStoreError> {
        validate_identity(provider, subject)?;
        let key = identity_key(provider, subject);
        if let Some(identity) = self.identities.get(&key) {
            let user = self
                .users
                .get(&identity.user_id)
                .expect("identity state must not contain orphaned identities")
                .clone();
            return Ok((user, identity.clone()));
        }
        let now = Utc::now();
        let user = User {
            id: UserId::new(),
            display_name: normalize_display_name(display_name),
            created_at: now,
            updated_at: now,
        };
        let identity = UserIdentity {
            provider: provider.to_owned(),
            subject: subject.to_owned(),
            user_id: user.id,
            created_at: now,
        };
        self.users.insert(user.id, user.clone());
        self.identities.insert(key, identity.clone());
        Ok((user, identity))
    }

    fn link_identity(
        &mut self,
        user_id: UserId,
        provider: &str,
        subject: &str,
    ) -> Result<UserIdentity, IdentityStoreError> {
        validate_identity(provider, subject)?;
        if !self.users.contains_key(&user_id) {
            return Err(IdentityStoreError::UserNotFound(user_id));
        }
        let key = identity_key(provider, subject);
        if let Some(existing) = self.identities.get(&key) {
            if existing.user_id == user_id {
                return Ok(existing.clone());
            }
            return Err(IdentityStoreError::IdentityAlreadyLinked {
                provider: provider.to_owned(),
                subject: subject.to_owned(),
            });
        }
        let identity = UserIdentity {
            provider: provider.to_owned(),
            subject: subject.to_owned(),
            user_id,
            created_at: Utc::now(),
        };
        self.identities.insert(key, identity.clone());
        Ok(identity)
    }
}

fn validate_identity(provider: &str, subject: &str) -> Result<(), IdentityStoreError> {
    if provider.trim().is_empty() || subject.trim().is_empty() {
        return Err(IdentityStoreError::EmptyIdentity);
    }
    Ok(())
}
fn normalize_display_name(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

#[derive(Clone, Default)]
pub struct InMemoryUserIdentityStore {
    state: Arc<Mutex<IdentityState>>,
}
impl UserIdentityStore for InMemoryUserIdentityStore {
    fn resolve_or_create(
        &self,
        provider: &str,
        subject: &str,
        display_name: Option<&str>,
    ) -> Result<(User, UserIdentity), IdentityStoreError> {
        Ok(self
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .resolve_or_create(provider, subject, display_name)?)
    }
    fn get_user(&self, user_id: UserId) -> Result<Option<User>, IdentityStoreError> {
        Ok(self
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .users
            .get(&user_id)
            .cloned())
    }
    fn find_identity(
        &self,
        provider: &str,
        subject: &str,
    ) -> Result<Option<UserIdentity>, IdentityStoreError> {
        Ok(self
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .identities
            .get(&identity_key(provider, subject))
            .cloned())
    }
    fn link_identity(
        &self,
        user_id: UserId,
        provider: &str,
        subject: &str,
    ) -> Result<UserIdentity, IdentityStoreError> {
        self.state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .link_identity(user_id, provider, subject)
    }
}

/// JSON-file-backed identity store. Writes are atomic and survive process restarts.
/// Like ConversationStore, concurrent writers in multiple processes are not supported.
pub struct FileUserIdentityStore {
    path: PathBuf,
    state: Mutex<IdentityState>,
}
impl FileUserIdentityStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, IdentityStoreError> {
        let path = path.into();
        let state = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => IdentityState::default(),
            Err(error) => return Err(error.into()),
        };
        Ok(Self {
            path,
            state: Mutex::new(state),
        })
    }

    fn update<T>(
        &self,
        operation: impl FnOnce(&mut IdentityState) -> Result<T, IdentityStoreError>,
    ) -> Result<T, IdentityStoreError> {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        let mut updated = serde_json::from_slice::<IdentityState>(&serde_json::to_vec(&*state)?)?;
        let result = operation(&mut updated)?;
        let bytes = serde_json::to_vec_pretty(&updated)?;
        atomic_write(&self.path, &bytes)?;
        *state = updated;
        Ok(result)
    }
}
impl UserIdentityStore for FileUserIdentityStore {
    fn resolve_or_create(
        &self,
        provider: &str,
        subject: &str,
        display_name: Option<&str>,
    ) -> Result<(User, UserIdentity), IdentityStoreError> {
        self.update(|state| state.resolve_or_create(provider, subject, display_name))
    }
    fn get_user(&self, user_id: UserId) -> Result<Option<User>, IdentityStoreError> {
        Ok(self
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .users
            .get(&user_id)
            .cloned())
    }
    fn find_identity(
        &self,
        provider: &str,
        subject: &str,
    ) -> Result<Option<UserIdentity>, IdentityStoreError> {
        Ok(self
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .identities
            .get(&identity_key(provider, subject))
            .cloned())
    }
    fn link_identity(
        &self,
        user_id: UserId,
        provider: &str,
        subject: &str,
    ) -> Result<UserIdentity, IdentityStoreError> {
        self.update(|state| state.link_identity(user_id, provider, subject))
    }
}

fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), io::Error> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
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
    use super::*;

    fn temporary_file() -> PathBuf {
        std::env::temp_dir().join(format!("eme-identities-{}.json", Uuid::new_v4()))
    }

    #[test]
    fn telegram_identity_resolves_to_the_same_user_after_reopening_store() {
        let path = temporary_file();
        let (first_user, first_identity) = {
            let store = FileUserIdentityStore::open(&path).unwrap();
            store
                .resolve_or_create("telegram", "123456", Some("Javier"))
                .unwrap()
        };
        let reopened = FileUserIdentityStore::open(&path).unwrap();
        let (second_user, second_identity) = reopened
            .resolve_or_create("telegram", "123456", Some("Changed name"))
            .unwrap();
        assert_eq!(first_user.id, second_user.id);
        assert_eq!(first_identity, second_identity);
        assert_eq!(second_user.display_name.as_deref(), Some("Javier"));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn one_external_identity_cannot_be_linked_to_two_users() {
        let store = InMemoryUserIdentityStore::default();
        let (first, _) = store.resolve_or_create("telegram", "42", None).unwrap();
        let (second, _) = store
            .resolve_or_create("google", "person@example.com", None)
            .unwrap();
        assert!(matches!(
            store.link_identity(second.id, "telegram", "42"),
            Err(IdentityStoreError::IdentityAlreadyLinked { .. })
        ));
        assert_eq!(
            store
                .find_identity("telegram", "42")
                .unwrap()
                .unwrap()
                .user_id,
            first.id
        );
    }

    #[test]
    fn a_user_can_have_multiple_external_identities() {
        let store = InMemoryUserIdentityStore::default();
        let (user, _) = store.resolve_or_create("telegram", "42", None).unwrap();
        store
            .link_identity(user.id, "google", "person@example.com")
            .unwrap();
        assert_eq!(
            store
                .find_identity("google", "person@example.com")
                .unwrap()
                .unwrap()
                .user_id,
            user.id
        );
    }
}
