//! Supported catalogue item kinds.
use serde::{Deserialize, Serialize};

/// A movie or episodic television series.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Movie,
    TvShow,
}
