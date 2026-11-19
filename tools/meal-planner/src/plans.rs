//! Mutable weekly meal plans and deterministic input validation.
use crate::MealType;
use chrono::{Datelike, Days, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanParticipant {
    pub member_id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MealAssignment {
    pub id: i64,
    pub date: NaiveDate,
    pub meal_type: MealType,
    pub participants: Vec<PlanParticipant>,
    #[serde(default)]
    pub recipe_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeeklyPlanDraft {
    pub assignments: Vec<MealAssignment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeeklyPlan {
    pub id: i64,
    pub family_id: String,
    pub week_start: NaiveDate,
    pub assignments: Vec<MealAssignment>,
    pub created_at: i64,
}

impl WeeklyPlanDraft {
    pub(crate) fn validate(self, week_start: NaiveDate) -> Result<Self, String> {
        if week_start.weekday() != Weekday::Mon {
            return Err("week_start must be a Monday".into());
        }
        let week_end = week_start
            .checked_add_days(Days::new(6))
            .ok_or_else(|| "week_start is outside the supported date range".to_owned())?;
        let mut ids = std::collections::HashSet::new();
        for assignment in &self.assignments {
            if assignment.id <= 0 || !ids.insert(assignment.id) {
                return Err("assignment IDs must be positive and unique within a plan".into());
            }
            if assignment.date < week_start || assignment.date > week_end {
                return Err("every assignment date must belong to the plan week".into());
            }
            if assignment.participants.is_empty() {
                return Err("every assignment must include at least one participant".into());
            }
            let mut members = std::collections::HashSet::new();
            for participant in &assignment.participants {
                if participant.member_id <= 0 || participant.name.trim().is_empty() {
                    return Err("assignment participants require a valid member and name".into());
                }
                if !members.insert(participant.member_id) {
                    return Err("an assignment cannot contain a participant more than once".into());
                }
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
    #[test]
    fn validates_week_boundary_and_monday_start() {
        let monday = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
        let assignment = MealAssignment {
            id: 1,
            date: monday,
            meal_type: MealType::Lunch,
            participants: vec![PlanParticipant {
                member_id: 1,
                name: "Alex".into(),
            }],
            recipe_ids: vec![],
        };
        assert!(
            WeeklyPlanDraft {
                assignments: vec![assignment.clone()]
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
        assert!(
            WeeklyPlanDraft {
                assignments: vec![assignment.clone(), assignment]
            }
            .validate(monday)
            .is_err()
        );
    }
}
