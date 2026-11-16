//! Tests for run lifecycle transitions.

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
