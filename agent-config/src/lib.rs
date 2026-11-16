//! Shared configuration infrastructure for the AI Platform workspace.
//!
//! This crate provides common, reusable primitives for loading TOML configuration
//! files and collections of configuration files from directories. It also defines
//! structured, path-aware errors so that configuration-owning crates can report
//! parsing, validation, and filesystem problems consistently.
//!
//! The crate deliberately does not define application-specific configuration
//! schemas or decide what a particular configuration means. Those schemas and
//! their semantic validation belong to the crates that own the corresponding
//! domain. This keeps the shared loader small while allowing each subsystem to
//! evolve its configuration independently.

mod error;
mod toml;

#[cfg(test)]
mod tests;

pub use error::ConfigError;
pub use toml::{read_toml, read_toml_dir};
