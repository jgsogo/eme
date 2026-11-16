use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use cron::Schedule;
use uuid::Uuid;

#[derive(Clone)]
pub(crate) struct ScheduledTask {
    pub id: Uuid,
    pub user_id: String,
    pub name: String,
    pub agent: String,
    pub prompt: String,
    pub schedule: Option<Schedule>,
    pub timezone: Tz,
    pub next_run: DateTime<Utc>,
}
