//! In-memory family invitation persistence.

mod in_memory_family_invitation_store;
mod state;

pub use in_memory_family_invitation_store::InMemoryFamilyInvitationStore;

#[cfg(test)]
mod tests;
