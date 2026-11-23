//! Atomic JSON-file-backed identity store.

use super::{
    IdentityState, IdentityStoreError, User, UserId, UserIdentity, UserIdentityStore, identity_key,
};
use chrono::NaiveDate;
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};
use uuid::Uuid;

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
    fn create_user(
        &self,
        display_name: &str,
        birthday: Option<NaiveDate>,
    ) -> Result<User, IdentityStoreError> {
        self.update(|state| state.create_user(display_name, birthday))
    }
    fn update_profile(
        &self,
        user_id: UserId,
        display_name: &str,
        birthday: Option<NaiveDate>,
    ) -> Result<User, IdentityStoreError> {
        self.update(|state| state.update_profile(user_id, display_name, birthday))
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
    fn update_display_name(
        &self,
        user_id: UserId,
        display_name: &str,
    ) -> Result<User, IdentityStoreError> {
        self.update(|state| state.update_display_name(user_id, display_name))
    }
    fn update_timezone(
        &self,
        user_id: UserId,
        timezone: Option<&str>,
    ) -> Result<User, IdentityStoreError> {
        self.update(|state| state.update_timezone(user_id, timezone))
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
    fn find_identity_for_user(
        &self,
        provider: &str,
        user_id: UserId,
    ) -> Result<Option<UserIdentity>, IdentityStoreError> {
        Ok(self
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .identities
            .values()
            .find(|identity| identity.provider == provider && identity.user_id == user_id)
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
