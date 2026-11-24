//! Background execution of one-time and recurring tasks.

mod scheduled_task;
pub use scheduled_task::ScheduledTask;
mod task_scheduler;
#[cfg(test)]
mod tests;

pub use task_scheduler::TaskScheduler;
#[cfg(test)]
use task_scheduler::{next_occurrence, submission_retry_delay};
