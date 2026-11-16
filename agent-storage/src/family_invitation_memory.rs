//! In-memory family invitation persistence.

mod in_memory_family_invitation_store;
mod state;

use state::State;
#[cfg(test)]
mod tests;

pub use in_memory_family_invitation_store::InMemoryFamilyInvitationStore;
