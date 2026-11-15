use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use cron::Schedule;

pub(crate) struct ScheduledTask {
    pub name: String,
    pub agent: String,
    pub prompt: String,
    pub schedule: Option<Schedule>,
    pub timezone: Tz,
    pub next_run: DateTime<Utc>,
}
