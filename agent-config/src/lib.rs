//! Shared configuration loading primitives and structured configuration errors.

mod error;
mod toml;

#[cfg(test)]
mod tests;

pub use error::ConfigError;
pub use toml::{read_toml, read_toml_dir};
