//! Process-local identity store.

use super::{
    IdentityState, IdentityStoreError, User, UserId, UserIdentity, UserIdentityStore, identity_key,
};
use chrono::NaiveDate;
use std::sync::{Arc, Mutex};

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
    fn create_user(
        &self,
        display_name: &str,
        birthday: Option<NaiveDate>,
    ) -> Result<User, IdentityStoreError> {
        self.state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .create_user(display_name, birthday)
    }
    fn update_profile(
        &self,
        user_id: UserId,
        display_name: &str,
        birthday: Option<NaiveDate>,
    ) -> Result<User, IdentityStoreError> {
        self.state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .update_profile(user_id, display_name, birthday)
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
        self.state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .update_display_name(user_id, display_name)
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
        self.state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .link_identity(user_id, provider, subject)
    }
}
