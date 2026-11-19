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
    pub const VALIDATE: &str = "meal_planner_validate.toml";
    pub const SAVE: &str = "meal_planner_save_draft.toml";
    pub const CONFIRM: &str = "meal_planner_confirm.toml";
}

pub fn register_tools(
    registry: &mut ToolRegistry,
    store: MealPlannerStore,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data");
    let app = MealPlannerApplication::new(store);
    registry.register_from_file(
        data.join(definitions::CONTEXT),
        GetPlanningContext(app.clone()),
    )?;
    registry.register_from_file(data.join(definitions::PROPOSE), ProposePlan(app.clone()))?;
    registry.register_from_file(data.join(definitions::VALIDATE), ValidatePlan(app.clone()))?;
    registry.register_from_file(data.join(definitions::SAVE), SavePlanDraft(app.clone()))?;
    registry.register_from_file(data.join(definitions::CONFIRM), ConfirmPlan(app))?;
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
struct ProposalArgs {
    week_start: NaiveDate,
    assignments: Vec<crate::MealAssignment>,
}

impl ProposalArgs {
    fn into_draft(self) -> (NaiveDate, WeeklyPlanDraft) {
        (
            self.week_start,
            WeeklyPlanDraft {
                assignments: self.assignments,
            },
        )
    }
}

struct GetPlanningContext(MealPlannerApplication);
struct ProposePlan(MealPlannerApplication);
struct ValidatePlan(MealPlannerApplication);
struct SavePlanDraft(MealPlannerApplication);
struct ConfirmPlan(MealPlannerApplication);

#[async_trait]
impl Tool for GetPlanningContext {
    async fn execute(&self, context: &ToolContext, arguments: Value) -> Result<String, ToolError> {
        let (owner_id, families) = user_scope(context)?;
        let args: WeekArgs = serde_json::from_value(arguments).map_err(|error| {
            ToolError::Execution(format!("invalid planning-context arguments: {error}"))
        })?;
        let value = self
            .0
            .planning_context(&owner_id, &families, args.week_start)
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        output(&value)
    }
}

#[async_trait]
impl Tool for ProposePlan {
    async fn execute(&self, context: &ToolContext, arguments: Value) -> Result<String, ToolError> {
        let (owner_id, families) = user_scope(context)?;
        let args: ProposalArgs = serde_json::from_value(arguments)
            .map_err(|error| ToolError::Execution(format!("invalid plan proposal: {error}")))?;
        let (week_start, draft) = args.into_draft();
        let proposal = self
            .0
            .propose(&owner_id, &families, week_start, draft)
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        output(&proposal)
    }
}

#[async_trait]
impl Tool for ValidatePlan {
    async fn execute(&self, context: &ToolContext, arguments: Value) -> Result<String, ToolError> {
        let (owner_id, families) = user_scope(context)?;
        let args: ProposalArgs = serde_json::from_value(arguments).map_err(|error| {
            ToolError::Execution(format!("invalid plan validation request: {error}"))
        })?;
        let (week_start, draft) = args.into_draft();
        let validation = self
            .0
            .validate(&owner_id, &families, week_start, &draft)
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        output(&validation)
    }
}

#[async_trait]
impl Tool for SavePlanDraft {
    async fn execute(&self, context: &ToolContext, arguments: Value) -> Result<String, ToolError> {
        let (owner_id, families) = user_scope(context)?;
        let args: ProposalArgs = serde_json::from_value(arguments)
            .map_err(|error| ToolError::Execution(format!("invalid plan draft: {error}")))?;
        let (week_start, draft) = args.into_draft();
        let validation = self
            .0
            .validate(&owner_id, &families, week_start, &draft)
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        if !validation.valid {
            return output(&json!({"saved": false, "confirmed": false, "validation": validation}));
        }
        let plan = self
            .0
            .save_draft(&owner_id, &families, week_start, draft)
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        output(&json!({"saved": true, "confirmed": false, "plan": plan}))
    }
}

#[async_trait]
impl Tool for ConfirmPlan {
    async fn execute(&self, context: &ToolContext, arguments: Value) -> Result<String, ToolError> {
        let (owner_id, families) = user_scope(context)?;
        let args: WeekArgs = serde_json::from_value(arguments)
            .map_err(|error| ToolError::Execution(format!("invalid plan confirmation: {error}")))?;
        let plan = self
            .0
            .confirm_saved_draft(&owner_id, &families, args.week_start)
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        output(&json!({"confirmed": true, "plan": plan}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_runtime::Tool;

    fn monday() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 16).unwrap()
    }

    #[test]
    fn registers_every_tool_from_its_toml_definition() {
        let mut registry = ToolRegistry::new();
        register_tools(&mut registry, MealPlannerStore::open_in_memory().unwrap()).unwrap();
        for name in [
            "meals.planner.get_context",
            "meals.planner.propose",
            "meals.planner.validate",
            "meals.planner.save_draft",
            "meals.planner.confirm",
        ] {
            assert!(registry.contains(name), "missing registered tool: {name}");
        }
    }

    #[tokio::test]
    async fn tools_require_authenticated_context() {
        let store = MealPlannerStore::open_in_memory().unwrap();
        let tool = GetPlanningContext(MealPlannerApplication::new(store));
        let error = tool
            .execute(&ToolContext::default(), json!({"week_start":"2026-11-16"}))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("authenticated user"));
    }

    #[tokio::test]
    async fn proposal_tool_does_not_persist_and_save_tool_only_saves_draft() {
        let store = MealPlannerStore::open_in_memory().unwrap();
        let member = store.create_family_member("Alex").unwrap();
        let app = MealPlannerApplication::new(store.clone());
        let arguments = json!({
            "week_start": monday(),
            "assignments": [{
                "id": 1, "date": monday(), "meal_type": "lunch",
                "participants": [{"member_id": member.id, "name": "Alex"}],
                "arrangement": "home", "recipe_ids": [], "school_menu_date": null, "notes": null
            }]
        });
        let context = ToolContext::new("user-1");
        let proposal = ProposePlan(app.clone())
            .execute(&context, arguments.clone())
            .await
            .unwrap();
        assert!(proposal.contains("\"valid\": true"));
        assert!(store.get_weekly_plan(monday()).unwrap().is_none());
        let saved = SavePlanDraft(app.clone())
            .execute(&context, arguments)
            .await
            .unwrap();
        assert!(saved.contains("\"saved\": true"));
        assert_eq!(
            store.get_weekly_plan(monday()).unwrap().unwrap().status,
            crate::WeeklyPlanStatus::Draft
        );
        let confirmed = ConfirmPlan(app)
            .execute(&context, json!({"week_start":"2026-11-16"}))
            .await
            .unwrap();
        assert!(confirmed.contains("\"confirmed\": true"));
    }

    #[tokio::test]
    async fn invalid_proposal_returns_structured_feedback_without_saving() {
        let store = MealPlannerStore::open_in_memory().unwrap();
        let app = MealPlannerApplication::new(store.clone());
        let value = ProposePlan(app)
            .execute(
                &ToolContext::new("user-1"),
                json!({
                    "week_start":"2026-11-16",
                    "assignments":[{
                        "id":1,"date":"2026-11-16","meal_type":"lunch",
                        "participants":[{"member_id":999,"name":"Unknown"}],
                        "arrangement":"home","recipe_id":null,"school_menu_date":null,"notes":null
                    }]
                }),
            )
            .await
            .unwrap();
        assert!(value.contains("participant_exists"));
        assert!(store.get_weekly_plan(monday()).unwrap().is_none());
    }
}
