//! Pure household entities and deterministic meal participation resolution.
use chrono::{Datelike, Days, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};

/// Stable household member identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyMember {
    pub id: i64,
    pub name: String,
    pub active: bool,
}

/// Meal categories supported by the initial planner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MealType {
    Breakfast,
    Lunch,
    Dinner,
}
impl MealType {
    pub const ALL: [Self; 3] = [Self::Breakfast, Self::Lunch, Self::Dinner];
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Breakfast => "breakfast",
            Self::Lunch => "lunch",
            Self::Dinner => "dinner",
        }
    }
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "breakfast" => Some(Self::Breakfast),
            "lunch" => Some(Self::Lunch),
            "dinner" => Some(Self::Dinner),
            _ => None,
        }
    }
}

/// Explicit arrangement for a meal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MealArrangement {
    Home,
    School,
    Away,
    NotParticipating,
}
impl MealArrangement {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::School => "school",
            Self::Away => "away",
            Self::NotParticipating => "not_participating",
        }
    }
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "home" => Some(Self::Home),
            "school" => Some(Self::School),
            "away" => Some(Self::Away),
            "not_participating" => Some(Self::NotParticipating),
            _ => None,
        }
    }
}

/// Weekday index: Monday is zero and Sunday is six.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum MealWeekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}
impl MealWeekday {
    pub fn from_date(date: NaiveDate) -> Self {
        match date.weekday() {
            Weekday::Mon => Self::Monday,
            Weekday::Tue => Self::Tuesday,
            Weekday::Wed => Self::Wednesday,
            Weekday::Thu => Self::Thursday,
            Weekday::Fri => Self::Friday,
            Weekday::Sat => Self::Saturday,
            Weekday::Sun => Self::Sunday,
        }
    }
    pub(crate) fn from_number(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Monday),
            1 => Some(Self::Tuesday),
            2 => Some(Self::Wednesday),
            3 => Some(Self::Thursday),
            4 => Some(Self::Friday),
            5 => Some(Self::Saturday),
            6 => Some(Self::Sunday),
            _ => None,
        }
    }
}

/// Recurring rule with inclusive optional effective dates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecurringParticipationRule {
    pub id: i64,
    pub member_id: i64,
    pub weekday: MealWeekday,
    pub meal_type: MealType,
    pub arrangement: MealArrangement,
    pub effective_from: Option<NaiveDate>,
    pub effective_until: Option<NaiveDate>,
}
impl RecurringParticipationRule {
    pub fn applies_on(&self, date: NaiveDate) -> bool {
        self.weekday == MealWeekday::from_date(date)
            && self.effective_from.is_none_or(|from| date >= from)
            && self.effective_until.is_none_or(|until| date <= until)
    }
}

/// One-off date-specific override.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MealException {
    pub id: i64,
    pub member_id: i64,
    pub date: NaiveDate,
    pub meal_type: MealType,
    pub arrangement: MealArrangement,
}

/// Source of a configured arrangement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParticipationSource {
    RecurringRule,
    DateException,
}

/// Missing configuration differs from an explicit not-participating choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParticipationStatus {
    Unconfigured,
    Configured {
        arrangement: MealArrangement,
        source: ParticipationSource,
    },
}

/// Resolved arrangement for one member, meal and local calendar date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MealParticipation {
    pub date: NaiveDate,
    pub meal_type: MealType,
    pub member: FamilyMember,
    pub status: ParticipationStatus,
}

/// Stateless resolver independent of persistence, HTTP and the agent runtime.
pub struct ParticipationResolver;
impl ParticipationResolver {
    pub fn resolve_date(
        date: NaiveDate,
        members: &[FamilyMember],
        rules: &[RecurringParticipationRule],
        exceptions: &[MealException],
    ) -> Vec<MealParticipation> {
        resolve_date(date, members, rules, exceptions)
    }
    pub fn resolve_week(
        start: NaiveDate,
        members: &[FamilyMember],
        rules: &[RecurringParticipationRule],
        exceptions: &[MealException],
    ) -> Vec<MealParticipation> {
        resolve_week(start, members, rules, exceptions)
    }
}

/// Resolve all active members for all meals. Exceptions win. Overlapping rules
/// use the latest effective start (unbounded first), then the greatest rule ID.
pub fn resolve_date(
    date: NaiveDate,
    members: &[FamilyMember],
    rules: &[RecurringParticipationRule],
    exceptions: &[MealException],
) -> Vec<MealParticipation> {
    let mut output = Vec::new();
    for member in members.iter().filter(|member| member.active) {
        for meal_type in MealType::ALL {
            let exception = exceptions.iter().find(|item| {
                item.member_id == member.id && item.date == date && item.meal_type == meal_type
            });
            let status = if let Some(item) = exception {
                ParticipationStatus::Configured {
                    arrangement: item.arrangement,
                    source: ParticipationSource::DateException,
                }
            } else {
                rules
                    .iter()
                    .filter(|rule| {
                        rule.member_id == member.id
                            && rule.meal_type == meal_type
                            && rule.applies_on(date)
                    })
                    .max_by_key(|rule| (rule.effective_from, rule.id))
                    .map_or(ParticipationStatus::Unconfigured, |rule| {
                        ParticipationStatus::Configured {
                            arrangement: rule.arrangement,
                            source: ParticipationSource::RecurringRule,
                        }
                    })
            };
            output.push(MealParticipation {
                date,
                meal_type,
                member: member.clone(),
                status,
            });
        }
    }
    output
}

/// Resolve seven consecutive local calendar dates beginning at start.
pub fn resolve_week(
    start: NaiveDate,
    members: &[FamilyMember],
    rules: &[RecurringParticipationRule],
    exceptions: &[MealException],
) -> Vec<MealParticipation> {
    (0..7)
        .filter_map(|offset| start.checked_add_days(Days::new(offset)))
        .flat_map(|date| resolve_date(date, members, rules, exceptions))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }
    fn member() -> FamilyMember {
        FamilyMember {
            id: 1,
            name: "Alex".into(),
            active: true,
        }
    }
    fn rule(
        id: i64,
        arrangement: MealArrangement,
        from: Option<&str>,
        until: Option<&str>,
    ) -> RecurringParticipationRule {
        RecurringParticipationRule {
            id,
            member_id: 1,
            weekday: MealWeekday::Monday,
            meal_type: MealType::Lunch,
            arrangement,
            effective_from: from.map(date),
            effective_until: until.map(date),
        }
    }
    #[test]
    fn exception_overrides_rule_and_can_mean_not_participating() {
        let exception = MealException {
            id: 1,
            member_id: 1,
            date: date("2026-11-16"),
            meal_type: MealType::Lunch,
            arrangement: MealArrangement::NotParticipating,
        };
        let result = resolve_date(
            date("2026-11-16"),
            &[member()],
            &[rule(1, MealArrangement::School, None, None)],
            &[exception],
        );
        assert_eq!(
            result[1].status,
            ParticipationStatus::Configured {
                arrangement: MealArrangement::NotParticipating,
                source: ParticipationSource::DateException
            }
        );
    }
    #[test]
    fn latest_overlapping_rule_wins() {
        let result = resolve_date(
            date("2026-11-16"),
            &[member()],
            &[
                rule(1, MealArrangement::Home, Some("2026-01-01"), None),
                rule(2, MealArrangement::School, Some("2026-11-01"), None),
            ],
            &[],
        );
        assert_eq!(
            result[1].status,
            ParticipationStatus::Configured {
                arrangement: MealArrangement::School,
                source: ParticipationSource::RecurringRule
            }
        );
    }
    #[test]
    fn effective_boundaries_and_weekdays_are_respected() {
        let rules = [rule(
            1,
            MealArrangement::Away,
            Some("2026-11-16"),
            Some("2026-11-16"),
        )];
        assert!(matches!(
            resolve_date(date("2026-11-16"), &[member()], &rules, &[])[1].status,
            ParticipationStatus::Configured { .. }
        ));
        assert_eq!(
            resolve_date(date("2026-11-17"), &[member()], &rules, &[])[1].status,
            ParticipationStatus::Unconfigured
        );
    }
    #[test]
    fn missing_configuration_is_distinct_and_week_has_seven_days() {
        assert!(
            resolve_date(date("2026-11-16"), &[member()], &[], &[])
                .iter()
                .all(|item| item.status == ParticipationStatus::Unconfigured)
        );
        assert_eq!(
            resolve_week(date("2026-11-16"), &[member()], &[], &[]).len(),
            21
        );
    }
}
