//! Weekly plan snapshots and deterministic validation.
use crate::MealType;
use chrono::{Datelike, Days, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeeklyPlanStatus {
    Draft,
    Confirmed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanParticipant {
    pub member_id: i64,
    pub name: String,
}

/// Participants and recipes assigned to one meal slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MealAssignment {
    pub id: i64,
    pub date: NaiveDate,
    pub meal_type: MealType,
    pub participants: Vec<PlanParticipant>,
    /// Only home-prepared assignments may reference a family recipe.
    pub recipe_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeeklyPlanDraft {
    pub assignments: Vec<MealAssignment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeeklyPlan {
    pub id: i64,
    pub week_start: NaiveDate,
    pub revision: i64,
    pub status: WeeklyPlanStatus,
    pub assignments: Vec<MealAssignment>,
    pub created_at: i64,
    pub confirmed_at: Option<i64>,
}

impl WeeklyPlanDraft {
    pub(crate) fn validate(self, week_start: NaiveDate) -> Result<Self, String> {
        if week_start.weekday() != Weekday::Mon {
            return Err("week_start must be a Monday".into());
        }
        let week_end = week_start
            .checked_add_days(Days::new(6))
            .ok_or_else(|| "week_start is outside the supported date range".to_owned())?;
        let mut assignment_ids = Vec::new();
        for assignment in &self.assignments {
            if assignment.id <= 0 || assignment_ids.contains(&assignment.id) {
                return Err("assignment IDs must be positive and unique within a plan".into());
            }
            assignment_ids.push(assignment.id);
            if assignment.date < week_start || assignment.date > week_end {
                return Err("every assignment date must belong to the plan week".into());
            }
            if assignment.participants.is_empty() {
                return Err("every assignment must include at least one participant".into());
            }
            let mut seen = Vec::new();
            for participant in &assignment.participants {
                if participant.member_id <= 0 || participant.name.trim().is_empty() {
                    return Err("assignment participants require a valid member and name".into());
                }
                if seen.contains(&participant.member_id) {
                    return Err("an assignment cannot contain a participant more than once".into());
                }
                seen.push(participant.member_id);
            }
            if assignment.recipe_ids.iter().any(|id| *id <= 0) {
                return Err("recipe_ids must contain only positive IDs".into());
            }
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assignment() -> MealAssignment {
        MealAssignment {
            id: 1,
            date: NaiveDate::from_ymd_opt(2026, 11, 16).unwrap(),
            meal_type: MealType::Lunch,
            participants: vec![PlanParticipant {
                member_id: 1,
                name: "Alex".into(),
            }],
            recipe_ids: vec![],
        }
    }

    #[test]
    fn validates_week_boundary_and_monday_start() {
        let monday = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
        assert!(
            WeeklyPlanDraft {
                assignments: vec![assignment()]
            }
            .validate(monday)
            .is_ok()
        );
        assert!(
            WeeklyPlanDraft {
                assignments: vec![]
            }
            .validate(monday.succ_opt().unwrap())
            .is_err()
        );
    }

    #[test]
    fn assignments_accept_multiple_recipes() {
        let monday = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
        let mut value = assignment();
        value.recipe_ids = vec![4, 5];
        assert!(
            WeeklyPlanDraft {
                assignments: vec![value]
            }
            .validate(monday)
            .is_ok()
        );
    }

    #[test]
    fn assignments_cannot_include_duplicate_or_missing_participants() {
        let monday = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
        let mut value = assignment();
        value.participants.push(value.participants[0].clone());
        assert!(
            WeeklyPlanDraft {
                assignments: vec![value]
            }
            .validate(monday)
            .is_err()
        );
    }
}
