//! Explicit, independently registered agent tools for Meal Planner resources.
use std::{error::Error,path::PathBuf};
use agent_runtime::{Tool,ToolContext,ToolError,ToolRegistry};
use async_trait::async_trait;
use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::Value;
use crate::{MealPlannerStore,RecipeDraft,SchoolMenuDraft,WeeklyPlanDraft,NewShoppingListItem,ShoppingListItemUpdate};

#[derive(Clone,Copy)]
enum Action{
 RecipeCreate,RecipeGet,RecipeList,RecipeUpdate,RecipeDelete,
 SchoolCreate,SchoolGet,SchoolList,SchoolUpdate,SchoolDelete,
 PlanCreate,PlanGet,PlanList,PlanUpdate,PlanDelete,
 ShoppingGet,ShoppingGenerate,ShoppingAdd,ShoppingUpdate,ShoppingRemove,ShoppingRestore,
}
struct MealTool{store:MealPlannerStore,action:Action}
fn scope(ctx:&ToolContext)->Result<(String,String),ToolError>{
 let owner=ctx.user_id.as_deref().filter(|v|!v.trim().is_empty()).ok_or_else(||ToolError::Execution("Meal tools require an authenticated user context".into()))?.to_owned();
 let family=ctx.family.as_ref().map(|v|v.family_id.clone()).unwrap_or_else(||"legacy".into());
 Ok((owner,family))
}
fn decode<T:for<'de>Deserialize<'de>>(v:Value)->Result<T,ToolError>{serde_json::from_value(v).map_err(|e|ToolError::Execution(format!("invalid tool arguments: {e}")))}
fn encode(v:&impl serde::Serialize)->Result<String,ToolError>{serde_json::to_string_pretty(v).map_err(|e|ToolError::Execution(e.to_string()))}
fn failure(e:impl std::fmt::Display)->ToolError{ToolError::Execution(e.to_string())}
#[derive(Deserialize)]struct IdArgs{id:i64}
#[derive(Deserialize)]struct DateArgs{date:NaiveDate}
#[derive(Deserialize)]struct RangeArgs{start_date:NaiveDate,end_date:NaiveDate}
#[derive(Deserialize)]struct WeekArgs{week_start:NaiveDate}
#[derive(Deserialize)]struct PlanArgs{week_start:NaiveDate,assignments:Vec<crate::MealAssignment>}
#[derive(Deserialize)]struct SearchArgs{query:Option<String>}
#[derive(Deserialize)]struct ListIdArgs{list_id:i64}
#[derive(Deserialize)]struct ItemArgs{list_id:i64,item_id:i64}
#[async_trait]
impl Tool for MealTool{
 async fn execute(&self,ctx:&ToolContext,args:Value)->Result<String,ToolError>{
  let(owner,family)=scope(ctx)?;let families=vec![family.clone()];
  match self.action{
   Action::RecipeCreate=>{let mut d:RecipeDraft=decode(args)?;if d.family_id.is_none(){d.family_id=Some(family.clone())}encode(&self.store.create_recipe(&owner,d).map_err(failure)?)}
   Action::RecipeGet=>{let a:IdArgs=decode(args)?;encode(&self.store.get_recipe(a.id,&owner,&families).map_err(failure)?)}
   Action::RecipeList=>{let a:SearchArgs=decode(args)?;encode(&self.store.list_recipes(&owner,&families,a.query.as_deref()).map_err(failure)?)}
   Action::RecipeUpdate=>{#[derive(Deserialize)]struct A{id:i64,#[serde(flatten)]draft:RecipeDraft}let a:A=decode(args)?;encode(&self.store.update_recipe_for_family(a.id,&owner,&family,a.draft).map_err(failure)?)}
   Action::RecipeDelete=>{let a:IdArgs=decode(args)?;self.store.delete_recipe_for_family(a.id,&owner,&family).map_err(failure)?;Ok("{\"deleted\":true}".into())}
   Action::SchoolCreate=>{let d:SchoolMenuDraft=decode(args)?;if self.store.get_school_menu_for_family(&family,d.date).map_err(failure)?.is_some(){return Err(failure("a school menu already exists for this date; use meals.school_menu.update"))}encode(&self.store.save_school_menu_for_family(&family,d).map_err(failure)?)}
   Action::SchoolGet=>{let a:DateArgs=decode(args)?;encode(&self.store.get_school_menu_for_family(&family,a.date).map_err(failure)?)}
   Action::SchoolList=>{let a:RangeArgs=decode(args)?;encode(&self.store.list_school_menus_for_family(&family,a.start_date,a.end_date).map_err(failure)?)}
   Action::SchoolUpdate=>{let d:SchoolMenuDraft=decode(args)?;if self.store.get_school_menu_for_family(&family,d.date).map_err(failure)?.is_none(){return Err(failure("school menu does not exist; use meals.school_menu.create"))}encode(&self.store.save_school_menu_for_family(&family,d).map_err(failure)?)}
   Action::SchoolDelete=>{let a:DateArgs=decode(args)?;self.store.delete_school_menu_for_family(&family,a.date).map_err(failure)?;Ok("{\"deleted\":true}".into())}
   Action::PlanCreate=>{let a:PlanArgs=decode(args)?;encode(&self.store.create_weekly_plan_for_family(&family,a.week_start,WeeklyPlanDraft{assignments:a.assignments}).map_err(failure)?)}
   Action::PlanGet=>{let a:WeekArgs=decode(args)?;encode(&self.store.get_weekly_plan_for_family(&family,a.week_start).map_err(failure)?)}
   Action::PlanList=>{let a:RangeArgs=decode(args)?;encode(&self.store.list_weekly_plans_for_family(&family,a.start_date,a.end_date).map_err(failure)?)}
   Action::PlanUpdate=>{let a:PlanArgs=decode(args)?;encode(&self.store.update_weekly_plan_for_family(&family,a.week_start,WeeklyPlanDraft{assignments:a.assignments}).map_err(failure)?)}
   Action::PlanDelete=>{let a:WeekArgs=decode(args)?;self.store.delete_weekly_plan_for_family(&family,a.week_start).map_err(failure)?;Ok("{\"deleted\":true}".into())}
   Action::ShoppingGet=>{let a:RangeArgs=decode(args)?;encode(&self.store.get_shopping_list_for_range(&family,a.start_date,a.end_date).map_err(failure)?)}
   Action::ShoppingGenerate=>{let a:RangeArgs=decode(args)?;encode(&self.store.generate_shopping_list_for_range(&family,a.start_date,a.end_date).map_err(failure)?)}
   Action::ShoppingAdd=>{let a:ListIdArgs=decode(args)?;#[derive(Deserialize)]struct B{list_id:i64,name:String,quantity:Option<f64>,unit:Option<String>}let b:B=decode(args)?;if a.list_id!=b.list_id{return Err(failure("invalid shopping list id"))}encode(&self.store.add_shopping_list_item(b.list_id,NewShoppingListItem{name:b.name,quantity:b.quantity,unit:b.unit}).map_err(failure)?)}
   Action::ShoppingUpdate=>{#[derive(Deserialize)]struct A{list_id:i64,item_id:i64,name:String,quantity:Option<f64>,unit:Option<String>,purchased:bool}let a:A=decode(args)?;encode(&self.store.update_shopping_list_item(a.list_id,a.item_id,ShoppingListItemUpdate{name:a.name,quantity:a.quantity,unit:a.unit,purchased:a.purchased}).map_err(failure)?)}
   Action::ShoppingRemove=>{let a:ItemArgs=decode(args)?;encode(&self.store.remove_shopping_list_item(a.list_id,a.item_id).map_err(failure)?)}
   Action::ShoppingRestore=>{let a:ItemArgs=decode(args)?;encode(&self.store.restore_shopping_list_item(a.list_id,a.item_id).map_err(failure)?)}
  }
 }
}

pub fn register_tools(registry:&mut ToolRegistry,store:MealPlannerStore)->Result<(),Box<dyn Error+Send+Sync>>{
 let data=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data");
 let tools=[
 ("meal_recipe_create.toml",Action::RecipeCreate),("meal_recipe_get.toml",Action::RecipeGet),("meal_recipe_list.toml",Action::RecipeList),("meal_recipe_update.toml",Action::RecipeUpdate),("meal_recipe_delete.toml",Action::RecipeDelete),
 ("meal_school_menu_create.toml",Action::SchoolCreate),("meal_school_menu_get.toml",Action::SchoolGet),("meal_school_menu_list.toml",Action::SchoolList),("meal_school_menu_update.toml",Action::SchoolUpdate),("meal_school_menu_delete.toml",Action::SchoolDelete),
 ("meal_plan_create.toml",Action::PlanCreate),("meal_plan_get.toml",Action::PlanGet),("meal_plan_list.toml",Action::PlanList),("meal_plan_update.toml",Action::PlanUpdate),("meal_plan_delete.toml",Action::PlanDelete),
 ("meal_shopping_list_get.toml",Action::ShoppingGet),("meal_shopping_list_generate.toml",Action::ShoppingGenerate),("meal_shopping_list_add_item.toml",Action::ShoppingAdd),("meal_shopping_list_update_item.toml",Action::ShoppingUpdate),("meal_shopping_list_remove_item.toml",Action::ShoppingRemove),("meal_shopping_list_restore_item.toml",Action::ShoppingRestore)
 ];
 for(file,action)in tools{registry.register_from_file(data.join(file),MealTool{store:store.clone(),action})?;}
 Ok(())
}

#[cfg(test)]mod tests{
 use super::*;
 #[test]fn registers_explicit_resource_tools(){let mut r=ToolRegistry::new();register_tools(&mut r,MealPlannerStore::open_in_memory().unwrap()).unwrap();for name in ["meals.recipe.create","meals.recipe.get","meals.recipe.list","meals.recipe.update","meals.recipe.delete","meals.school_menu.create","meals.school_menu.get","meals.school_menu.list","meals.school_menu.update","meals.school_menu.delete","meals.plan.create","meals.plan.get","meals.plan.list","meals.plan.update","meals.plan.delete","meals.shopping_list.get","meals.shopping_list.generate","meals.shopping_list.add_item","meals.shopping_list.update_item","meals.shopping_list.remove_item","meals.shopping_list.restore_item"]{assert!(r.contains(name),"missing tool {name}")}assert!(!r.contains("meals.plan.validate"));assert!(!r.contains("meals.plan.confirm"));}
}
