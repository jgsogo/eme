//! Errors caused by invalid run lifecycle transitions.

use super::RunStatus;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
#[error("invalid run state transition: {from:?} -> {to:?}")]
pub struct InvalidRunTransition {
    pub from: RunStatus,
    pub to: RunStatus,
}
