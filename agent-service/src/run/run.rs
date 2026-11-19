//! Defines the service-level state and transitions of an individual run.

use super::{InvalidRunTransition, RunId, RunStatus};
use std::time::SystemTime;
use uuid::Uuid;

/// Service-level representation of a submitted run.
///
/// This type deliberately contains no HTTP or persistence-specific concepts.
#[derive(Debug, Clone)]
pub struct Run {
    pub id: RunId,
    pub agent: String,
    pub input: String,
    pub conversation_id: Option<Uuid>,
    pub status: RunStatus,
    pub created_at: SystemTime,
    pub started_at: Option<SystemTime>,
    pub finished_at: Option<SystemTime>,
    pub response: Option<String>,
    pub error: Option<String>,
}

impl Run {
    pub fn new(agent: impl Into<String>, input: impl Into<String>) -> Self {
        Self {
            id: RunId::new(),
            agent: agent.into(),
            input: input.into(),
            conversation_id: None,
            status: RunStatus::Queued,
            created_at: SystemTime::now(),
            started_at: None,
            finished_at: None,
            response: None,
            error: None,
        }
    }

    pub fn mark_running(&mut self) -> Result<(), InvalidRunTransition> {
        self.transition(RunStatus::Running)?;
        self.started_at = Some(SystemTime::now());
        Ok(())
    }

    pub fn complete(&mut self, response: Option<String>) -> Result<(), InvalidRunTransition> {
        self.transition(RunStatus::Completed)?;
        self.response = response;
        self.finished_at = Some(SystemTime::now());
        Ok(())
    }

    pub fn fail(&mut self, error: impl Into<String>) -> Result<(), InvalidRunTransition> {
        self.transition(RunStatus::Failed)?;
        self.error = Some(error.into());
        self.finished_at = Some(SystemTime::now());
        Ok(())
    }

    pub fn cancel(&mut self) -> Result<(), InvalidRunTransition> {
        self.transition(RunStatus::Cancelled)?;
        self.finished_at = Some(SystemTime::now());
        Ok(())
    }

    pub fn time_out(&mut self) -> Result<(), InvalidRunTransition> {
        self.transition(RunStatus::TimedOut)?;
        self.finished_at = Some(SystemTime::now());
        Ok(())
    }

    pub fn reach_max_iterations(&mut self) -> Result<(), InvalidRunTransition> {
        self.transition(RunStatus::MaxIterationsReached)?;
        self.finished_at = Some(SystemTime::now());
        Ok(())
    }

    fn transition(&mut self, to: RunStatus) -> Result<(), InvalidRunTransition> {
        let valid = match (self.status, to) {
            (RunStatus::Queued, RunStatus::Running | RunStatus::Cancelled) => true,
            (
                RunStatus::Running,
                RunStatus::Completed
                | RunStatus::Failed
                | RunStatus::Cancelled
                | RunStatus::TimedOut
                | RunStatus::MaxIterationsReached,
            ) => true,
            _ => false,
        };

        if !valid {
            return Err(InvalidRunTransition {
                from: self.status,
                to,
            });
        }

        self.status = to;
        Ok(())
    }
}
