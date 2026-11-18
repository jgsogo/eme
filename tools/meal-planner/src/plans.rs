//! Weekly plan snapshots and deterministic validation.
use crate::{MealArrangement, MealType};
use chrono::{Days, Datelike, NaiveDate, Weekday};
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

/// One group of participants sharing the same arrangement in a meal slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MealAssignment {
    pub id: i64,
    pub date: NaiveDate,
    pub meal_type: MealType,
    pub participants: Vec<PlanParticipant>,
    pub arrangement: MealArrangement,
    /// Only home-prepared assignments may reference a family recipe.
    pub recipe_id: Option<i64>,
    /// School assignments refer to the menu for this local calendar date.
    pub school_menu_date: Option<NaiveDate>,
    pub notes: Option<String>,
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
        let week_end = week_start.checked_add_days(Days::new(6))
            .ok_or_else(|| "week_start is outside the supported date range".to_owned())?;
        for assignment in &self.assignments {
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
            match assignment.arrangement {
                MealArrangement::Home => {
                    if assignment.school_menu_date.is_some() {
                        return Err("home-prepared assignments cannot reference a school menu".into());
                    }
                }
                MealArrangement::School => {
                    if assignment.recipe_id.is_some() {
                        return Err("school assignments cannot reference a family recipe".into());
                    }
                    if assignment.school_menu_date != Some(assignment.date) {
                        return Err("school assignments must reference the menu for their date".into());
                    }
                }
                MealArrangement::Away | MealArrangement::NotParticipating => {
                    if assignment.recipe_id.is_some() || assignment.school_menu_date.is_some() {
                        return Err("away and non-participating assignments cannot reference recipes or school menus".into());
                    }
                }
            }
            if assignment.recipe_id.is_some_and(|id| id <= 0) {
                return Err("recipe_id must be positive".into());
            }
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assignment(arrangement: MealArrangement) -> MealAssignment {
        MealAssignment {
            id: 1,
            date: NaiveDate::from_ymd_opt(2026, 11, 16).unwrap(),
            meal_type: MealType::Lunch,
            participants: vec![PlanParticipant { member_id: 1, name: "Alex".into() }],
            arrangement,
            recipe_id: None,
            school_menu_date: (arrangement == MealArrangement::School)
                .then(|| NaiveDate::from_ymd_opt(2026, 11, 16).unwrap()),
            notes: None,
        }
    }

    #[test]
    fn validates_week_boundary_and_monday_start() {
        let monday = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
        assert!(WeeklyPlanDraft { assignments: vec![assignment(MealArrangement::Home)] }
            .validate(monday).is_ok());
        assert!(WeeklyPlanDraft { assignments: vec![] }
            .validate(monday.succ_opt().unwrap()).is_err());
    }

    #[test]
    fn school_meals_reference_menu_and_never_a_recipe() {
        let monday = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
        let mut value = assignment(MealArrangement::School);
        value.recipe_id = Some(4);
        assert!(WeeklyPlanDraft { assignments: vec![value] }.validate(monday).is_err());
    }

    #[test]
    fn assignments_cannot_include_duplicate_or_missing_participants() {
        let monday = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
        let mut value = assignment(MealArrangement::Home);
        value.participants.push(value.participants[0].clone());
        assert!(WeeklyPlanDraft { assignments: vec![value] }.validate(monday).is_err());
    }
}
