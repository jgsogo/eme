//! Shared configuration loading primitives and error types.
//!
//! Domain crates own their configuration schemas and semantic validation.
//! This crate provides consistent TOML file/directory loading and structured,
//! path-aware diagnostics.

mod error;
mod toml;

#[cfg(test)]
mod tests;

pub use error::ConfigError;
pub use toml::{read_toml, read_toml_dir};
