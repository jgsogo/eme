//! Background scheduling for persisted one-time and recurring tasks.

mod scheduled_task;
mod task_scheduler;
#[cfg(test)]
mod tests;

pub use scheduled_task::ScheduledTask;
pub use task_scheduler::TaskScheduler;
#[cfg(test)]
use task_scheduler::next_occurrence;
