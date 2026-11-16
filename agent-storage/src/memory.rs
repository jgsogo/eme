//! In-memory implementation of run, event, and message persistence.

mod in_memory_storage;
mod state;

pub use in_memory_storage::InMemoryStorage;

#[cfg(test)]
mod tests;
