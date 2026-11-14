//! Shared configuration loading primitives and error types.
//!
//! Domain crates own their configuration schemas and semantic validation.
//! This crate provides consistent TOML file/directory loading and structured,
//! path-aware diagnostics.

use std::{
    fs,
    io,
    path::{Path, PathBuf},
};

use serde::de::DeserializeOwned;

/// Errors produced while loading or validating configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Failed to access configuration path '{path}': {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("Invalid TOML in configuration file '{path}': {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("Invalid {kind} configuration at {path:?}: {message}")]
    Invalid {
        kind: String,
        path: Option<PathBuf>,
        message: String,
    },

    #[error("Duplicate {kind} '{name}' at {path:?}")]
    Duplicate {
        kind: String,
        name: String,
        path: Option<PathBuf>,
    },

    #[error("No {kind} configuration files found in '{path}'")]
    NoFilesFound { kind: String, path: PathBuf },
}

impl ConfigError {
    pub fn invalid(
        kind: impl Into<String>,
        path: Option<impl Into<PathBuf>>,
        message: impl Into<String>,
    ) -> Self {
        Self::Invalid {
            kind: kind.into(),
            path: path.map(Into::into),
            message: message.into(),
        }
    }

    pub fn duplicate(
        kind: impl Into<String>,
        name: impl Into<String>,
        path: Option<impl Into<PathBuf>>,
    ) -> Self {
        Self::Duplicate {
            kind: kind.into(),
            name: name.into(),
            path: path.map(Into::into),
        }
    }

    pub fn no_files(kind: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self::NoFilesFound {
            kind: kind.into(),
            path: path.into(),
        }
    }
}

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

/// Read TOML files from a directory in deterministic path order.
///
/// Non-TOML entries are ignored. An empty directory produces an empty vector;
/// callers decide whether an empty configuration set is valid for their domain.
pub fn read_toml_dir<T: DeserializeOwned>(
    directory: impl AsRef<Path>,
) -> Result<Vec<(PathBuf, T)>, ConfigError> {
    let directory = directory.as_ref();
    let entries = fs::read_dir(directory).map_err(|source| ConfigError::Io {
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

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use serde::Deserialize;

    use super::{ConfigError, read_toml, read_toml_dir};

    #[derive(Debug, Deserialize, PartialEq)]
    struct Fixture {
        name: String,
    }

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "agent-config-test-{}",
                uuid_placeholder()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn uuid_placeholder() -> String {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        format!("{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed))
    }

    #[test]
    fn reads_a_toml_file() {
        let directory = TestDirectory::new();
        let path = directory.0.join("valid.toml");
        fs::write(&path, "name = 'example'").unwrap();

        assert_eq!(
            read_toml::<Fixture>(&path).unwrap(),
            Fixture {
                name: "example".into()
            }
        );
    }

    #[test]
    fn reports_parse_errors_with_the_source_path() {
        let directory = TestDirectory::new();
        let path = directory.0.join("invalid.toml");
        fs::write(&path, "name = [").unwrap();

        let error = read_toml::<Fixture>(&path).unwrap_err();
        assert!(matches!(error, ConfigError::Parse { path: error_path, .. } if error_path == path));
    }

    #[test]
    fn loads_toml_directories_in_sorted_order_and_ignores_other_files() {
        let directory = TestDirectory::new();
        fs::write(directory.0.join("b.toml"), "name = 'b'").unwrap();
        fs::write(directory.0.join("a.toml"), "name = 'a'").unwrap();
        fs::write(directory.0.join("notes.txt"), "ignored").unwrap();

        let loaded = read_toml_dir::<Fixture>(&directory.0).unwrap();
        let names = loaded
            .into_iter()
            .map(|(_, fixture)| fixture.name)
            .collect::<Vec<_>>();

        assert_eq!(names, ["a", "b"]);
    }
}
