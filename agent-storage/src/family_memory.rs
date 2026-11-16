//! In-memory family and membership persistence.

mod in_memory_family_store;
mod state;
#[cfg(test)]
mod tests;

use state::State;

pub use in_memory_family_store::InMemoryFamilyStore;
