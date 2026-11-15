use std::{
    io,
    path::PathBuf,
};

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
