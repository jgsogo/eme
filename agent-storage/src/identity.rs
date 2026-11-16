//! User identity records and storage implementations.

mod file_user_identity_store;
mod identity_state;
mod identity_store_error;
mod in_memory_user_identity_store;
mod user;
mod user_id;
mod user_identity;
mod user_identity_store;

pub use file_user_identity_store::FileUserIdentityStore;
pub use identity_store_error::IdentityStoreError;
pub use in_memory_user_identity_store::InMemoryUserIdentityStore;
pub use user::User;
pub use user_id::UserId;
pub use user_identity::UserIdentity;
pub use user_identity_store::UserIdentityStore;

#[cfg(test)]
mod tests;
