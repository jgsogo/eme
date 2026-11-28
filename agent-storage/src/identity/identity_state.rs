//! Shared identity state and invariant-preserving operations.

use super::{IdentityStoreError, User, UserId, UserIdentity};
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Default, Serialize, Deserialize)]
pub(super) struct IdentityState {
    pub(super) users: HashMap<UserId, User>,
    pub(super) identities: HashMap<String, UserIdentity>,
}

pub(super) fn identity_key(provider: &str, subject: &str) -> String {
    format!("{provider}\\0{subject}")
}

impl IdentityState {
    pub(super) fn resolve_or_create(
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
            birthday: None,
            timezone: None,
            one_shot_agent: None,
            one_shot_family_id: None,
            one_shot_prompt: None,
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

    pub(super) fn create_user(
        &mut self,
        display_name: &str,
        birthday: Option<NaiveDate>,
    ) -> Result<User, IdentityStoreError> {
        let display_name = validate_display_name(display_name)?;
        let now = Utc::now();
        let user = User {
            id: UserId::new(),
            display_name: Some(display_name),
            birthday,
            timezone: None,
            one_shot_agent: None,
            one_shot_family_id: None,
            one_shot_prompt: None,
            created_at: now,
            updated_at: now,
        };
        self.users.insert(user.id, user.clone());
        Ok(user)
    }

    pub(super) fn update_profile(
        &mut self,
        user_id: UserId,
        display_name: &str,
        birthday: Option<NaiveDate>,
    ) -> Result<User, IdentityStoreError> {
        let display_name = validate_display_name(display_name)?;
        let user = self
            .users
            .get_mut(&user_id)
            .ok_or(IdentityStoreError::UserNotFound(user_id))?;
        user.display_name = Some(display_name);
        user.birthday = birthday;
        user.updated_at = Utc::now();
        Ok(user.clone())
    }

    pub(super) fn update_display_name(
        &mut self,
        user_id: UserId,
        display_name: &str,
    ) -> Result<User, IdentityStoreError> {
        let display_name = validate_display_name(display_name)?;
        let user = self
            .users
            .get_mut(&user_id)
            .ok_or(IdentityStoreError::UserNotFound(user_id))?;
        user.display_name = Some(display_name);
        user.updated_at = Utc::now();
        Ok(user.clone())
    }

    pub(super) fn update_timezone(
        &mut self,
        user_id: UserId,
        timezone: Option<&str>,
    ) -> Result<User, IdentityStoreError> {
        let timezone = timezone
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        if timezone
            .as_ref()
            .is_some_and(|value| value.len() > 64 || !value.contains('/'))
        {
            return Err(IdentityStoreError::InvalidTimezone);
        }
        let user = self
            .users
            .get_mut(&user_id)
            .ok_or(IdentityStoreError::UserNotFound(user_id))?;
        user.timezone = timezone;
        user.updated_at = Utc::now();
        Ok(user.clone())
    }

    pub(super) fn update_one_shot_config(
        &mut self,
        user_id: UserId,
        agent: Option<&str>,
        family_id: Option<&str>,
        prompt: Option<&str>,
    ) -> Result<User, IdentityStoreError> {
        let user = self
            .users
            .get_mut(&user_id)
            .ok_or(IdentityStoreError::UserNotFound(user_id))?;
        user.one_shot_agent = agent.map(str::trim).filter(|value| !value.is_empty()).map(str::to_owned);
        user.one_shot_family_id = family_id.map(str::trim).filter(|value| !value.is_empty()).map(str::to_owned);
        user.one_shot_prompt = prompt.map(str::trim).filter(|value| !value.is_empty()).map(str::to_owned);
        user.updated_at = Utc::now();
        Ok(user.clone())
    }

    pub(super) fn link_identity(
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

fn validate_display_name(value: &str) -> Result<String, IdentityStoreError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 80 {
        return Err(IdentityStoreError::InvalidDisplayName);
    }
    Ok(value.to_owned())
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
