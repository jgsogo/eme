use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::de::DeserializeOwned;

use crate::error::ConfigError;

/// Read and deserialize one TOML configuration file.
pub fn read_toml<T: DeserializeOwned>(path: impl AsRef<Path>) -> Result<T, ConfigError> {
    let path = path.as_ref();
    let contents = fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    toml::from_str(&contents).map_err(|source| ConfigError::Parse {
        path: path.to_path_buf(),
        source,
    })
}
