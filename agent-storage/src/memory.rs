//! In-memory implementation of run, event, and message persistence.

mod in_memory_storage;
mod state;
#[cfg(test)]
mod tests;

use state::State;

pub use in_memory_storage::InMemoryStorage;
