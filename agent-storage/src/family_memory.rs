//! In-memory family and membership persistence.

mod in_memory_family_store;
mod state;

use state::State;
#[cfg(test)]
mod tests;

pub use in_memory_family_store::InMemoryFamilyStore;
