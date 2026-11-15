use std::time::SystemTime;
use super::{InvalidRunTransition, RunId, RunStatus};

/// Service-level representation of a submitted run.
///
/// This type deliberately contains no HTTP or persistence-specific concepts.
#[derive(Debug, Clone)]
pub struct Run {
    pub id: RunId,
    pub agent: String,
    pub input: String,
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

#[cfg(test)]
mod tests {
    use super::{Run, RunStatus};

    #[test]
    fn new_run_has_a_stable_id_and_starts_queued() {
        let run = Run::new("assistant", "Hello");

        assert_eq!(run.status, RunStatus::Queued);
        assert!(run.started_at.is_none());
        assert!(run.finished_at.is_none());
        assert_eq!(run.id, run.id);
    }

    #[test]
    fn run_moves_through_a_valid_lifecycle() {
        let mut run = Run::new("assistant", "Hello");

        run.mark_running().unwrap();
        assert_eq!(run.status, RunStatus::Running);
        assert!(run.started_at.is_some());

        run.complete(Some("Hi".into())).unwrap();
        assert_eq!(run.status, RunStatus::Completed);
        assert_eq!(run.response.as_deref(), Some("Hi"));
        assert!(run.finished_at.is_some());
    }

    #[test]
    fn queued_runs_can_be_cancelled_but_cannot_be_completed() {
        let mut run = Run::new("assistant", "Hello");

        assert!(run.complete(None).is_err());
        run.cancel().unwrap();

        assert_eq!(run.status, RunStatus::Cancelled);
        assert!(run.finished_at.is_some());
        assert!(run.mark_running().is_err());
    }

    #[test]
    fn terminal_runs_cannot_transition_again() {
        let mut run = Run::new("assistant", "Hello");
        run.mark_running().unwrap();
        run.fail("model unavailable").unwrap();

        assert!(run.cancel().is_err());
        assert_eq!(run.status, RunStatus::Failed);
        assert_eq!(run.error.as_deref(), Some("model unavailable"));
    }
}
