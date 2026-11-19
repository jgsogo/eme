//! Application context for AI-assisted meal planning.
use chrono::{Datelike, Days, NaiveDate, Weekday};
use serde::Serialize;
use crate::{FamilyMember, MealArrangement, MealPlannerError, MealPlannerStore, MealType, Recipe, SchoolMenuEntry, WeeklyPlan};

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
#[derive(Clone)]
pub struct MealPlannerApplication { store: MealPlannerStore }
impl MealPlannerApplication {
    pub fn new(store: MealPlannerStore) -> Self { Self { store } }
    pub fn planning_context(&self, owner_id:&str, families:&[String], week_start:NaiveDate) -> Result<PlanningContext,MealPlannerError> {
        if week_start.weekday()!=Weekday::Mon { return Err(MealPlannerError::InvalidInput("week_start must be a Monday".into())); }
        let family_id=families.first().map(String::as_str).unwrap_or("legacy");
        let week_end=week_start.checked_add_days(Days::new(6)).ok_or_else(||MealPlannerError::InvalidInput("week_start is outside the supported date range".into()))?;
        let participation=self.store.resolve_meal_week_for_family(family_id,week_start)?.into_iter().map(|entry|{
            let (arrangement,source)=match entry.status {
                crate::ParticipationStatus::Unconfigured=>(None,None),
                crate::ParticipationStatus::Configured{arrangement,source}=>{
                    let source=match source {crate::ParticipationSource::RecurringRule=>"recurring_rule",crate::ParticipationSource::DateException=>"date_exception"};
                    (Some(arrangement),Some(source.to_owned()))
                }
            };
            PlanningParticipation{date:entry.date,meal_type:entry.meal_type,member_id:entry.member.id,member_name:entry.member.name,arrangement,source}
        }).collect();
        Ok(PlanningContext{
            week_start,week_end,
            members:self.store.list_family_members_for_family(family_id)?,
            participation,
            recipes:self.store.list_recipes(owner_id,families,None)?,
            school_menus:self.store.list_school_menus_for_family(family_id,week_start,week_end)?,
            current_plan:self.store.get_weekly_plan_for_family(family_id,week_start)?,
        })
    }
}
