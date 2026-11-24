//! A task prepared for execution by the scheduler.

use agent_storage::FamilyId;
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use cron::Schedule;
use uuid::Uuid;

#[derive(Clone)]
pub struct ScheduledTask {
    pub id: Uuid,
    pub user_id: String,
    pub name: String,
    pub agent: String,
    pub prompt: String,
    pub family_id: Option<FamilyId>,
    pub schedule: Option<Schedule>,
    pub timezone: Tz,
    /// The cron occurrence currently awaiting successful submission.
    pub scheduled_for: DateTime<Utc>,
    /// Next time the scheduler should attempt submission (may be a retry deadline).
    pub next_run: DateTime<Utc>,
    pub retry_attempts: u32,
}
