//! TOML deserialization from individual files and deterministic directory scans.

mod directory;
mod file;

pub use directory::read_toml_dir;
pub use file::read_toml;
