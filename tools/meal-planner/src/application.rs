//! Application use cases for AI-assisted meal planning.
//!
//! Proposals are untrusted values, not persisted records. The application
//! validates them against current household and calendar data. Saving a draft
//! and confirming it are separate, explicit operations.
use chrono::{Datelike, Days, NaiveDate, Weekday};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

use crate::{
    FamilyMember, MealArrangement, MealPlannerError, MealPlannerStore, MealType, Recipe,
    SchoolMenuEntry, SchoolMenuStatus, WeeklyPlan, WeeklyPlanDraft, WeeklyPlanStatus,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanValidationIssue {
    pub constraint: String,
    pub severity: ValidationSeverity,
    pub message: String,
    pub date: Option<NaiveDate>,
    pub meal_type: Option<MealType>,
    pub assignment_id: Option<i64>,
    pub member_id: Option<i64>,
    pub recipe_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanValidation {
    pub valid: bool,
    pub issues: Vec<PlanValidationIssue>,
}

impl PlanValidation {
    fn new(issues: Vec<PlanValidationIssue>) -> Self {
        let valid = !issues
            .iter()
            .any(|item| matches!(item.severity, ValidationSeverity::Error));
        Self { valid, issues }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanningParticipation {
    pub date: NaiveDate,
    pub meal_type: MealType,
    pub member_id: i64,
    pub member_name: String,
    pub arrangement: Option<MealArrangement>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanningContext {
    pub week_start: NaiveDate,
    pub week_end: NaiveDate,
    pub members: Vec<FamilyMember>,
    pub participation: Vec<PlanningParticipation>,
    pub recipes: Vec<Recipe>,
    pub school_menus: Vec<SchoolMenuEntry>,
    pub current_plan: Option<WeeklyPlan>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanProposal {
    pub week_start: NaiveDate,
    pub draft: WeeklyPlanDraft,
    pub validation: PlanValidation,
}

#[derive(Clone)]
pub struct MealPlannerApplication {
    store: MealPlannerStore,
}

impl MealPlannerApplication {
    pub fn new(store: MealPlannerStore) -> Self {
        Self { store }
    }

    pub fn planning_context(
        &self,
        owner_id: &str,
        families: &[String],
        week_start: NaiveDate,
    ) -> Result<PlanningContext, MealPlannerError> {
        if week_start.weekday() != Weekday::Mon {
            return Err(MealPlannerError::InvalidInput(
                "week_start must be a Monday".into(),
            ));
        }
        let week_end = week_start.checked_add_days(Days::new(6)).ok_or_else(|| {
            MealPlannerError::InvalidInput("week_start is outside the supported date range".into())
        })?;
        let participation = self
            .store
            .resolve_meal_week(week_start)?
            .into_iter()
            .map(|entry| {
                let (arrangement, source) = match entry.status {
                    crate::ParticipationStatus::Unconfigured => (None, None),
                    crate::ParticipationStatus::Configured {
                        arrangement,
                        source,
                    } => {
                        let source = match source {
                            crate::ParticipationSource::RecurringRule => "recurring_rule",
                            crate::ParticipationSource::DateException => "date_exception",
                        };
                        (Some(arrangement), Some(source.to_owned()))
                    }
                };
                PlanningParticipation {
                    date: entry.date,
                    meal_type: entry.meal_type,
                    member_id: entry.member.id,
                    member_name: entry.member.name,
                    arrangement,
                    source,
                }
            })
            .collect();
        Ok(PlanningContext {
            week_start,
            week_end,
            members: self.store.list_family_members()?,
            participation,
            recipes: self.store.list_recipes(owner_id, families, None)?,
            school_menus: self.store.list_school_menus(week_start, week_end)?,
            current_plan: self.store.get_weekly_plan(week_start)?,
        })
    }

    /// Build a candidate and deterministic feedback without writing to storage.
    pub fn propose(
        &self,
        owner_id: &str,
        families: &[String],
        week_start: NaiveDate,
        draft: WeeklyPlanDraft,
    ) -> Result<PlanProposal, MealPlannerError> {
        let validation = self.validate(owner_id, families, week_start, &draft)?;
        Ok(PlanProposal {
            week_start,
            draft,
            validation,
        })
    }

    pub fn validate(
        &self,
        owner_id: &str,
        families: &[String],
        week_start: NaiveDate,
        draft: &WeeklyPlanDraft,
    ) -> Result<PlanValidation, MealPlannerError> {
        let members = self.store.list_family_members()?;
        let recipes = self.store.list_recipes(owner_id, families, None)?;
        let menus = match week_start.checked_add_days(Days::new(6)) {
            Some(end) => self.store.list_school_menus(week_start, end)?,
            None => Vec::new(),
        };
        let member_by_id = members
            .iter()
            .map(|item| (item.id, item))
            .collect::<HashMap<_, _>>();
        let recipe_ids = recipes.iter().map(|item| item.id).collect::<HashSet<_>>();
        let menu_by_date = menus
            .iter()
            .map(|item| (item.draft.date, item))
            .collect::<HashMap<_, _>>();
        let mut issues = Vec::new();
        let mut assignment_ids = HashSet::new();
        let mut slot_participants = HashSet::new();

        if week_start.weekday() != Weekday::Mon {
            issues.push(issue(
                "week_start_monday",
                ValidationSeverity::Error,
                "The plan week must start on a Monday.",
                None,
                None,
                None,
                None,
                None,
            ));
        }
        let week_end = week_start.checked_add_days(Days::new(6));

        for assignment in &draft.assignments {
            let date = Some(assignment.date);
            let meal = Some(assignment.meal_type);
            let id = Some(assignment.id);
            if assignment.id <= 0 || !assignment_ids.insert(assignment.id) {
                issues.push(issue(
                    "assignment_id_unique",
                    ValidationSeverity::Error,
                    "Assignment IDs must be positive and unique within a plan.",
                    date,
                    meal,
                    id,
                    None,
                    None,
                ));
            }
            if week_end.is_none_or(|end| assignment.date < week_start || assignment.date > end) {
                issues.push(issue(
                    "assignment_within_week",
                    ValidationSeverity::Error,
                    "The assignment date must belong to the selected Monday-to-Sunday week.",
                    date,
                    meal,
                    id,
                    None,
                    None,
                ));
            }
            if assignment.participants.is_empty() {
                issues.push(issue(
                    "assignment_has_participants",
                    ValidationSeverity::Error,
                    "Every assignment must include at least one participant.",
                    date,
                    meal,
                    id,
                    None,
                    None,
                ));
            }
            let mut local_members = HashSet::new();
            for participant in &assignment.participants {
                if participant.member_id <= 0 || participant.name.trim().is_empty() {
                    issues.push(issue("participant_identity_valid", ValidationSeverity::Error, "Each participant needs a positive member ID and a non-empty snapshot name.", date, meal, id, Some(participant.member_id), None));
                }
                if !local_members.insert(participant.member_id) {
                    issues.push(issue(
                        "participant_unique_in_assignment",
                        ValidationSeverity::Error,
                        "A participant appears more than once in this assignment.",
                        date,
                        meal,
                        id,
                        Some(participant.member_id),
                        None,
                    ));
                }
                if !slot_participants.insert((
                    assignment.date,
                    assignment.meal_type,
                    participant.member_id,
                )) {
                    issues.push(issue("participant_single_assignment_per_slot", ValidationSeverity::Error, "A participant cannot have multiple assignments for the same date and meal.", date, meal, id, Some(participant.member_id), None));
                }
                match member_by_id.get(&participant.member_id) {
                    None => issues.push(issue(
                        "participant_exists",
                        ValidationSeverity::Error,
                        "The referenced household member does not exist.",
                        date,
                        meal,
                        id,
                        Some(participant.member_id),
                        None,
                    )),
                    Some(member) if !member.active => issues.push(issue(
                        "participant_active",
                        ValidationSeverity::Error,
                        "The referenced household member is inactive.",
                        date,
                        meal,
                        id,
                        Some(participant.member_id),
                        None,
                    )),
                    _ => {}
                }
            }
            match assignment.arrangement {
                MealArrangement::Home => {
                    if assignment.school_menu_date.is_some() {
                        issues.push(issue(
                            "home_without_school_menu",
                            ValidationSeverity::Error,
                            "Home-prepared assignments cannot reference a school menu.",
                            date,
                            meal,
                            id,
                            None,
                            None,
                        ));
                    }
                }
                MealArrangement::School => {
                    if assignment.recipe_id.is_some() {
                        issues.push(issue(
                            "school_without_recipe",
                            ValidationSeverity::Error,
                            "School assignments cannot reference a family recipe.",
                            date,
                            meal,
                            id,
                            None,
                            assignment.recipe_id,
                        ));
                    }
                    if assignment.school_menu_date != Some(assignment.date) {
                        issues.push(issue(
                            "school_menu_matches_date",
                            ValidationSeverity::Error,
                            "A school assignment must reference the menu for its own date.",
                            date,
                            meal,
                            id,
                            None,
                            None,
                        ));
                    } else {
                        match menu_by_date.get(&assignment.date) {
                            None => issues.push(issue("school_menu_recorded", ValidationSeverity::Warning, "No school menu is recorded for this date; this is not evidence that the school is closed.", date, meal, id, None, None)),
                            Some(menu) if menu.draft.status == SchoolMenuStatus::Closed => issues.push(issue("school_menu_not_closed", ValidationSeverity::Error, "The recorded school menu says the school is closed on this date.", date, meal, id, None, None)),
                            Some(menu) if menu.draft.status != SchoolMenuStatus::Known => issues.push(issue("school_menu_known", ValidationSeverity::Warning, "The school menu is not published or its status is unknown.", date, meal, id, None, None)),
                            _ => {}
                        }
                    }
                }
                MealArrangement::Away | MealArrangement::NotParticipating => {
                    if assignment.recipe_id.is_some() || assignment.school_menu_date.is_some() {
                        issues.push(issue("away_without_references", ValidationSeverity::Error, "Away and not-participating assignments cannot reference recipes or school menus.", date, meal, id, None, assignment.recipe_id));
                    }
                }
            }
            if let Some(recipe_id) = assignment.recipe_id {
                if recipe_id <= 0 || !recipe_ids.contains(&recipe_id) {
                    issues.push(issue(
                        "recipe_exists",
                        ValidationSeverity::Error,
                        "The referenced recipe does not exist.",
                        date,
                        meal,
                        id,
                        None,
                        Some(recipe_id),
                    ));
                }
            }
        }
        Ok(PlanValidation::new(issues))
    }

    /// Persist a validated proposal as a draft. This operation never confirms it.
    pub fn save_draft(
        &self,
        owner_id: &str,
        families: &[String],
        week_start: NaiveDate,
        draft: WeeklyPlanDraft,
    ) -> Result<WeeklyPlan, MealPlannerError> {
        let validation = self.validate(owner_id, families, week_start, &draft)?;
        if !validation.valid {
            return Err(MealPlannerError::InvalidInput(
                "the proposal has validation errors; correct it before saving".into(),
            ));
        }
        match self.store.get_weekly_plan(week_start)? {
            None => self.store.create_weekly_plan(week_start, draft),
            Some(plan) => match plan.status {
                WeeklyPlanStatus::Draft => self.store.update_weekly_plan(week_start, draft),
                WeeklyPlanStatus::Confirmed => {
                    self.store.revise_weekly_plan(week_start)?;
                    self.store.update_weekly_plan(week_start, draft)
                }
            },
        }
    }

    /// Confirm only the already-saved latest draft after revalidating it.
    pub fn confirm_saved_draft(
        &self,
        owner_id: &str,
        families: &[String],
        week_start: NaiveDate,
    ) -> Result<WeeklyPlan, MealPlannerError> {
        let plan = self
            .store
            .get_weekly_plan(week_start)?
            .ok_or(MealPlannerError::NotFound)?;
        if plan.status != WeeklyPlanStatus::Draft {
            return Err(MealPlannerError::Conflict(
                "only a saved draft can be confirmed".into(),
            ));
        }
        let draft = WeeklyPlanDraft {
            assignments: plan.assignments.clone(),
        };
        if !self.validate(owner_id, families, week_start, &draft)?.valid {
            return Err(MealPlannerError::InvalidInput(
                "the saved draft has validation errors; correct it before confirming".into(),
            ));
        }
        self.store.confirm_weekly_plan(week_start)
    }
}

fn issue(
    constraint: &str,
    severity: ValidationSeverity,
    message: &str,
    date: Option<NaiveDate>,
    meal_type: Option<MealType>,
    assignment_id: Option<i64>,
    member_id: Option<i64>,
    recipe_id: Option<i64>,
) -> PlanValidationIssue {
    PlanValidationIssue {
        constraint: constraint.into(),
        severity,
        message: message.into(),
        date,
        meal_type,
        assignment_id,
        member_id,
        recipe_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MealAssignment, PlanParticipant};

    fn monday() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 16).unwrap()
    }
    fn draft(member_id: i64) -> WeeklyPlanDraft {
        WeeklyPlanDraft {
            assignments: vec![MealAssignment {
                id: 1,
                date: monday(),
                meal_type: MealType::Lunch,
                participants: vec![PlanParticipant {
                    member_id,
                    name: "Alex".into(),
                }],
                arrangement: MealArrangement::Home,
                recipe_id: None,
                school_menu_date: None,
                notes: None,
            }],
        }
    }

    #[test]
    fn proposal_is_validated_but_never_persisted() {
        let store = MealPlannerStore::open_in_memory().unwrap();
        let member = store.create_family_member("Alex").unwrap();
        let app = MealPlannerApplication::new(store.clone());
        assert!(
            app.propose("user-1", &[], monday(), draft(member.id))
                .unwrap()
                .validation
                .valid
        );
        assert!(store.get_weekly_plan(monday()).unwrap().is_none());
    }

    #[test]
    fn validation_returns_structured_assignment_and_reference_violations() {
        let store = MealPlannerStore::open_in_memory().unwrap();
        let app = MealPlannerApplication::new(store);
        let mut proposal = draft(999);
        proposal.assignments[0].arrangement = MealArrangement::School;
        proposal.assignments[0].recipe_id = Some(123);
        let result = app.validate("user-1", &[], monday(), &proposal).unwrap();
        assert!(!result.valid);
        assert!(
            result
                .issues
                .iter()
                .any(|item| item.constraint == "participant_exists"
                    && item.assignment_id == Some(1)
                    && item.date == Some(monday()))
        );
        assert!(
            result
                .issues
                .iter()
                .any(|item| item.constraint == "school_without_recipe")
        );
        assert!(
            result
                .issues
                .iter()
                .any(|item| item.constraint == "school_menu_matches_date")
        );
        assert!(
            result
                .issues
                .iter()
                .any(|item| item.constraint == "recipe_exists")
        );
    }

    #[test]
    fn saving_a_draft_and_confirming_it_are_separate_operations() {
        let store = MealPlannerStore::open_in_memory().unwrap();
        let member = store.create_family_member("Alex").unwrap();
        let app = MealPlannerApplication::new(store.clone());
        assert_eq!(
            app.save_draft("user-1", &[], monday(), draft(member.id)).unwrap().status,
            WeeklyPlanStatus::Draft
        );
        assert_eq!(
            store.get_weekly_plan(monday()).unwrap().unwrap().status,
            WeeklyPlanStatus::Draft
        );
        assert_eq!(
            app.confirm_saved_draft("user-1", &[], monday()).unwrap().status,
            WeeklyPlanStatus::Confirmed
        );
        assert!(app.confirm_saved_draft("user-1", &[], monday()).is_err());
    }

    #[test]
    fn saving_after_confirmation_creates_a_new_draft_revision() {
        let store = MealPlannerStore::open_in_memory().unwrap();
        let member = store.create_family_member("Alex").unwrap();
        let app = MealPlannerApplication::new(store.clone());
        let first = app.save_draft("user-1", &[], monday(), draft(member.id)).unwrap();
        let confirmed = app.confirm_saved_draft("user-1", &[], monday()).unwrap();
        assert_eq!(confirmed.revision, first.revision);
        let mut revised = draft(member.id);
        revised.assignments[0].notes = Some("Updated after approval".into());
        let saved = app.save_draft("user-1", &[], monday(), revised).unwrap();
        assert_eq!(saved.revision, first.revision + 1);
        assert_eq!(saved.status, WeeklyPlanStatus::Draft);
        assert_eq!(
            saved.assignments[0].notes.as_deref(),
            Some("Updated after approval")
        );
        let revisions = store.list_weekly_plan_revisions(monday()).unwrap();
        assert_eq!(revisions.len(), 2);
        assert_eq!(revisions[1].status, WeeklyPlanStatus::Confirmed);
    }

    #[test]
    fn context_includes_members_and_existing_plan_state() {
        let store = MealPlannerStore::open_in_memory().unwrap();
        store.create_family_member("Alex").unwrap();
        let context = MealPlannerApplication::new(store)
            .planning_context("user-1", &[], monday())
            .unwrap();
        assert_eq!(context.members.len(), 1);
        assert!(context.current_plan.is_none());
    }
}
