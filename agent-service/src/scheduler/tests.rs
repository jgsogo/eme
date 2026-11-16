//! Tests for scheduled task execution.

use super::next_occurrence;
use chrono::{Duration, Timelike, Utc};
use cron::Schedule;
use std::str::FromStr;

#[test]
fn calculates_recurring_occurrences_in_requested_timezone() {
    let schedule = Schedule::from_str("0 0 8 * * *").unwrap();
    let timezone = "Europe/Madrid".parse().unwrap();
    let after = Utc::now();
    let next = next_occurrence(&schedule, timezone, after).unwrap();
    assert!(next > after);
    assert_eq!(next.with_timezone(&timezone).hour(), 8);
}

#[test]
fn skips_missed_occurrences() {
    let schedule = Schedule::from_str("0 0 8 * * *").unwrap();
    let after = Utc::now() + Duration::days(2);
    assert!(next_occurrence(&schedule, "UTC".parse().unwrap(), after).unwrap() > after);
}
