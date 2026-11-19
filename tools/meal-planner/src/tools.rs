//! Thin EME agent-tool adapters over MealPlannerApplication.
use std::{error::Error, path::PathBuf};

use agent_runtime::{Tool, ToolContext, ToolError, ToolRegistry};
use async_trait::async_trait;
use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{MealPlannerApplication, MealPlannerStore, WeeklyPlanDraft};

mod definitions {
    pub const CONTEXT: &str = "meal_planner_context.toml";
    pub const PROPOSE: &str = "meal_planner_propose.toml";
    pub const SAVE: &str = "meal_planner_save_draft.toml";
}

pub fn register_tools(
    registry: &mut ToolRegistry,
    store: MealPlannerStore,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data");
    let app = MealPlannerApplication::new(store);
    registry.register_from_file(data.join(definitions::CONTEXT), GetPlanningContext(app.clone()))?;
    registry.register_from_file(data.join(definitions::PROPOSE), ProposePlan(app.clone()))?;
    registry.register_from_file(data.join(definitions::SAVE), SavePlan(app))?;
    Ok(())
}

fn user_scope(context: &ToolContext) -> Result<(String, Vec<String>), ToolError> {
    let owner_id = context
        .user_id
        .as_deref()
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| {
            ToolError::Execution("Meal Planner tools require an authenticated user context".into())
        })?
        .to_owned();
    let families = context
        .family
        .as_ref()
        .map(|family| vec![family.family_id.clone()])
        .unwrap_or_default();
    Ok((owner_id, families))
}

fn output(value: &impl serde::Serialize) -> Result<String, ToolError> {
    serde_json::to_string_pretty(value).map_err(|error| ToolError::Execution(error.to_string()))
}

#[derive(Deserialize)]
struct WeekArgs {
    week_start: NaiveDate,
}

#[derive(Deserialize)]
struct PlanArgs {
    week_start: NaiveDate,
    assignments: Vec<crate::MealAssignment>,
}

impl PlanArgs {
    fn into_draft(self) -> (NaiveDate, WeeklyPlanDraft) {
        (self.week_start, WeeklyPlanDraft { assignments: self.assignments })
    }
}

struct GetPlanningContext(MealPlannerApplication);
struct ProposePlan(MealPlannerApplication);
struct SavePlan(MealPlannerApplication);

#[async_trait]
impl Tool for GetPlanningContext {
    async fn execute(&self, context: &ToolContext, arguments: Value) -> Result<String, ToolError> {
        let (owner_id, families) = user_scope(context)?;
        let args: WeekArgs = serde_json::from_value(arguments).map_err(|error| {
            ToolError::Execution(format!("invalid planning-context arguments: {error}"))
        })?;
        let value = self.0.planning_context(&owner_id, &families, args.week_start)
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        output(&value)
    }
}

#[async_trait]
impl Tool for ProposePlan {
    async fn execute(&self, context: &ToolContext, arguments: Value) -> Result<String, ToolError> {
        let (owner_id, families) = user_scope(context)?;
        let args: PlanArgs = serde_json::from_value(arguments)
            .map_err(|error| ToolError::Execution(format!("invalid plan proposal: {error}")))?;
        let (week_start, draft) = args.into_draft();
        let proposal = self.0.propose(&owner_id, &families, week_start, draft)
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        output(&proposal)
    }
}

#[async_trait]
impl Tool for SavePlan {
    async fn execute(&self, context: &ToolContext, arguments: Value) -> Result<String, ToolError> {
        let (owner_id, families) = user_scope(context)?;
        let args: PlanArgs = serde_json::from_value(arguments)
            .map_err(|error| ToolError::Execution(format!("invalid plan: {error}")))?;
        let (week_start, draft) = args.into_draft();
        let plan = self.0.save_draft(&owner_id, &families, week_start, draft)
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        output(&json!({"saved": true, "plan": plan}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_runtime::Tool;

    fn monday() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 16).unwrap()
    }

    fn arguments(member_id: i64) -> Value {
        json!({
            "week_start": monday(),
            "assignments": [{
                "id": 1, "date": monday(), "meal_type": "lunch",
                "participants": [{"member_id": member_id, "name": "Alex"}],
                "recipe_ids": []
            }]
        })
    }

    #[test]
    fn registers_the_supported_plan_tools() {
        let mut registry = ToolRegistry::new();
        register_tools(&mut registry, MealPlannerStore::open_in_memory().unwrap()).unwrap();
        for name in ["meals.plan.get", "meals.plan.propose", "meals.plan.save"] {
            assert!(registry.contains(name), "missing registered tool: {name}");
        }
        assert!(!registry.contains("meals.plan.validate"));
        assert!(!registry.contains("meals.plan.confirm"));
    }

    #[tokio::test]
    async fn tools_require_authenticated_context() {
        let tool = GetPlanningContext(MealPlannerApplication::new(
            MealPlannerStore::open_in_memory().unwrap(),
        ));
        let error = tool.execute(&ToolContext::default(), json!({"week_start":"2026-11-16"}))
            .await.unwrap_err();
        assert!(error.to_string().contains("authenticated user"));
    }

    #[tokio::test]
    async fn proposal_is_non_persistent_and_save_persists_directly() {
        let store = MealPlannerStore::open_in_memory().unwrap();
        let member = store.create_family_member("Alex").unwrap();
        let app = MealPlannerApplication::new(store.clone());
        let context = ToolContext::new("user-1");
        let proposal = ProposePlan(app.clone()).execute(&context, arguments(member.id)).await.unwrap();
        assert!(proposal.contains("\"valid\": true"));
        assert!(store.get_weekly_plan(monday()).unwrap().is_none());

        let saved = SavePlan(app).execute(&context, arguments(member.id)).await.unwrap();
        assert!(saved.contains("\"saved\": true"));
        assert!(store.get_weekly_plan(monday()).unwrap().is_some());
    }

    #[tokio::test]
    async fn invalid_plan_is_not_persisted() {
        let store = MealPlannerStore::open_in_memory().unwrap();
        let app = MealPlannerApplication::new(store.clone());
        let result = SavePlan(app).execute(
            &ToolContext::new("user-1"),
            json!({
                "week_start":"2026-11-16",
                "assignments":[{
                    "id":1,"date":"2026-11-16","meal_type":"lunch",
                    "participants":[{"member_id":999,"name":"Unknown"}],"recipe_ids":[]
                }]
            }),
        ).await;
        assert!(result.is_err());
        assert!(store.get_weekly_plan(monday()).unwrap().is_none());
    }
}
