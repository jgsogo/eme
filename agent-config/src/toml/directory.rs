use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;

use crate::{error::ConfigError, toml::file::read_toml};

/// Read TOML files from a directory in deterministic path order.
///
/// Non-TOML entries are ignored. An empty directory produces an empty vector;
/// callers decide whether an empty configuration set is valid for their domain.
pub fn read_toml_dir<T: DeserializeOwned>(
    directory: impl AsRef<Path>,
) -> Result<Vec<(PathBuf, T)>, ConfigError> {
    let directory = directory.as_ref();
    let entries = std::fs::read_dir(directory).map_err(|source| ConfigError::Io {
        path: directory.to_path_buf(),
        source,
    })?;

    let mut paths = entries
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|source| ConfigError::Io {
                    path: directory.to_path_buf(),
                    source,
                })
        })
        .collect::<Result<Vec<_>, _>>()?;

    paths.retain(|path| path.extension().and_then(|extension| extension.to_str()) == Some("toml"));
    paths.sort();

    paths
        .into_iter()
        .map(|path| read_toml(&path).map(|config| (path, config)))
        .collect()
}
