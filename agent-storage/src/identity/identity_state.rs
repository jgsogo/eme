//! Shared identity state and invariant-preserving operations.

use super::{IdentityStoreError, User, UserId, UserIdentity};
use chrono::Utc;
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
