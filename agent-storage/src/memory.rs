//! In-memory implementation of run, event, and message persistence.

mod in_memory_storage;
mod state;

use state::State;
#[cfg(test)]
mod tests;

pub use in_memory_storage::InMemoryStorage;
