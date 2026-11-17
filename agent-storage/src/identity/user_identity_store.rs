//! Operations for resolving and linking external identities.

use super::{IdentityStoreError, User, UserId, UserIdentity};

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
    fn update_display_name(&self, user_id: UserId, display_name: &str) -> Result<User, IdentityStoreError>;
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
