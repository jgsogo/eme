//! Authenticated HTTP API for the household recipe catalogue.
use actix_web::{
    HttpResponse, Responder, Scope, delete, get, post, put,
    web::{self, Data, Json, Path, Query},
};
use chrono::NaiveDate;
use std::{str::FromStr, sync::Arc};
use agent_service::FamilyService;
use agent_storage::{FamilyId, FileUserIdentityStore, UserIdentityStore, UserId};
use actix_web::HttpRequest;
use meal_planner::{
    MealPlannerError, MealPlannerStore, NewShoppingListItem, RecipeDraft, ShoppingListItemUpdate,
    WeeklyPlanDraft,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct SearchQuery {
    q: Option<String>,
}

fn error_response(error: MealPlannerError) -> HttpResponse {
    match error {
        MealPlannerError::NotFound => HttpResponse::NotFound().finish(),
        MealPlannerError::InvalidInput(message) => HttpResponse::BadRequest().body(message),
        MealPlannerError::Conflict(message) => HttpResponse::Conflict().body(message),
        _ => HttpResponse::InternalServerError().finish(),
    }
}

#[derive(Deserialize)]
struct CreateMemberRequest {
    name: String,
}

#[post("/members")]
async fn create_member(
    store: Data<MealPlannerStore>,
    body: Json<CreateMemberRequest>,
) -> impl Responder {
    match store.create_family_member(&body.name) {
        Ok(member) => HttpResponse::Created().json(member),
        Err(error) => error_response(error),
    }
}

#[get("/members")]
async fn list_members(store: Data<MealPlannerStore>) -> impl Responder {
    match store.list_family_members() {
        Ok(members) => HttpResponse::Ok().json(members),
        Err(error) => error_response(error),
    }
}

fn authenticated_user(request:&HttpRequest,auth:&crate::auth::TelegramAuth,identities:&FileUserIdentityStore)->Result<UserId,HttpResponse>{
 let owner=auth.owner_for_request(request).ok_or_else(||HttpResponse::Unauthorized().finish())?;
 let subject=owner.strip_prefix("telegram:").filter(|s|!s.is_empty()).ok_or_else(||HttpResponse::Unauthorized().finish())?;
 identities.find_identity("telegram",subject).map_err(|_|HttpResponse::InternalServerError().finish())?.map(|i|i.user_id).ok_or_else(||HttpResponse::Unauthorized().finish())
}
async fn family_ids(user:UserId,families:&FamilyService)->Result<Vec<String>,HttpResponse>{
 families.list_families(user).await.map(|fs|fs.into_iter().map(|f|f.id.to_string()).collect()).map_err(|_|HttpResponse::InternalServerError().finish())
}
fn validate_family(id:Option<&str>,available:&[String])->Result<(),HttpResponse>{
 if let Some(id)=id{FamilyId::from_str(id).map_err(|_|HttpResponse::BadRequest().body("invalid family id"))?;if !available.iter().any(|f|f==id){return Err(HttpResponse::NotFound().finish())}}Ok(())
}
#[get("/recipes")] async fn list_recipes(request:HttpRequest,auth:Data<crate::auth::TelegramAuth>,identities:Data<Arc<FileUserIdentityStore>>,families:Data<Arc<FamilyService>>,store:Data<MealPlannerStore>,query:Query<SearchQuery>)->impl Responder{
 let user=match authenticated_user(&request,&auth,&identities){Ok(v)=>v,Err(r)=>return r};let ids=match family_ids(user,&families).await{Ok(v)=>v,Err(r)=>return r};
 match store.list_recipes(&user.to_string(),&ids,query.q.as_deref()){Ok(v)=>HttpResponse::Ok().json(v),Err(e)=>error_response(e)}
}
#[post("/recipes")] async fn create_recipe(request:HttpRequest,auth:Data<crate::auth::TelegramAuth>,identities:Data<Arc<FileUserIdentityStore>>,families:Data<Arc<FamilyService>>,store:Data<MealPlannerStore>,body:Json<RecipeDraft>)->impl Responder{
 let user=match authenticated_user(&request,&auth,&identities){Ok(v)=>v,Err(r)=>return r};let ids=match family_ids(user,&families).await{Ok(v)=>v,Err(r)=>return r};if let Err(r)=validate_family(body.family_id.as_deref(),&ids){return r}
 match store.create_recipe(&user.to_string(),body.into_inner()){Ok(v)=>HttpResponse::Created().json(v),Err(e)=>error_response(e)}
}
#[get("/recipes/{id}")] async fn get_recipe(request:HttpRequest,auth:Data<crate::auth::TelegramAuth>,identities:Data<Arc<FileUserIdentityStore>>,families:Data<Arc<FamilyService>>,store:Data<MealPlannerStore>,id:Path<i64>)->impl Responder{
 let user=match authenticated_user(&request,&auth,&identities){Ok(v)=>v,Err(r)=>return r};let ids=match family_ids(user,&families).await{Ok(v)=>v,Err(r)=>return r};
 match store.get_recipe(id.into_inner(),&user.to_string(),&ids){Ok(v)=>HttpResponse::Ok().json(v),Err(e)=>error_response(e)}
}
#[put("/recipes/{id}")] async fn update_recipe(request:HttpRequest,auth:Data<crate::auth::TelegramAuth>,identities:Data<Arc<FileUserIdentityStore>>,families:Data<Arc<FamilyService>>,store:Data<MealPlannerStore>,id:Path<i64>,body:Json<RecipeDraft>)->impl Responder{
 let user=match authenticated_user(&request,&auth,&identities){Ok(v)=>v,Err(r)=>return r};let ids=match family_ids(user,&families).await{Ok(v)=>v,Err(r)=>return r};if let Err(r)=validate_family(body.family_id.as_deref(),&ids){return r}
 match store.update_recipe(id.into_inner(),&user.to_string(),body.into_inner()){Ok(v)=>HttpResponse::Ok().json(v),Err(e)=>error_response(e)}
}
#[delete("/recipes/{id}")] async fn delete_recipe(request:HttpRequest,auth:Data<crate::auth::TelegramAuth>,identities:Data<Arc<FileUserIdentityStore>>,store:Data<MealPlannerStore>,id:Path<i64>)->impl Responder{
 let user=match authenticated_user(&request,&auth,&identities){Ok(v)=>v,Err(r)=>return r};
 match store.delete_recipe(id.into_inner(),&user.to_string()){Ok(())=>HttpResponse::NoContent().finish(),Err(e)=>error_response(e)}
}
#[get("/plans/{week_start}")]
async fn get_weekly_plan(
    store: Data<MealPlannerStore>,
    week_start: Path<NaiveDate>,
) -> impl Responder {
    match store.get_weekly_plan(week_start.into_inner()) {
        Ok(Some(plan)) => HttpResponse::Ok().json(plan),
        Ok(None) => HttpResponse::NotFound().finish(),
        Err(error) => error_response(error),
    }
}

#[get("/plans/{week_start}/revisions")]
async fn list_weekly_plan_revisions(
    store: Data<MealPlannerStore>,
    week_start: Path<NaiveDate>,
) -> impl Responder {
    match store.list_weekly_plan_revisions(week_start.into_inner()) {
        Ok(plans) => HttpResponse::Ok().json(plans),
        Err(error) => error_response(error),
    }
}

#[post("/plans/{week_start}")]
async fn create_weekly_plan(
    store: Data<MealPlannerStore>,
    week_start: Path<NaiveDate>,
) -> impl Responder {
    let week_start = week_start.into_inner();
    match store.create_weekly_plan_from_rules(week_start) {
        Ok(plan) => HttpResponse::Created().json(plan),
        Err(error) => error_response(error),
    }
}

#[put("/plans/{week_start}")]
async fn update_weekly_plan(
    store: Data<MealPlannerStore>,
    week_start: Path<NaiveDate>,
    body: Json<WeeklyPlanDraft>,
) -> impl Responder {
    match store.update_weekly_plan(week_start.into_inner(), body.into_inner()) {
        Ok(plan) => HttpResponse::Ok().json(plan),
        Err(error) => error_response(error),
    }
}

#[post("/plans/{week_start}/confirm")]
async fn confirm_weekly_plan(
    store: Data<MealPlannerStore>,
    week_start: Path<NaiveDate>,
) -> impl Responder {
    match store.confirm_weekly_plan(week_start.into_inner()) {
        Ok(plan) => HttpResponse::Ok().json(plan),
        Err(error) => error_response(error),
    }
}

#[post("/plans/{week_start}/revise")]
async fn revise_weekly_plan(
    store: Data<MealPlannerStore>,
    week_start: Path<NaiveDate>,
) -> impl Responder {
    match store.revise_weekly_plan(week_start.into_inner()) {
        Ok(plan) => HttpResponse::Created().json(plan),
        Err(error) => error_response(error),
    }
}

#[get("/plans/{plan_id}/shopping-list")]
async fn get_shopping_list(store: Data<MealPlannerStore>, plan_id: Path<i64>) -> impl Responder {
    match store.get_shopping_list(plan_id.into_inner()) {
        Ok(Some(list)) => HttpResponse::Ok().json(list),
        Ok(None) => HttpResponse::NotFound().finish(),
        Err(error) => error_response(error),
    }
}

#[post("/plans/{plan_id}/shopping-list/generate")]
async fn generate_shopping_list(
    store: Data<MealPlannerStore>,
    plan_id: Path<i64>,
) -> impl Responder {
    match store.generate_shopping_list(plan_id.into_inner()) {
        Ok(list) => HttpResponse::Ok().json(list),
        Err(error) => error_response(error),
    }
}

#[post("/plans/{plan_id}/shopping-list/items")]
async fn add_shopping_list_item(
    store: Data<MealPlannerStore>,
    plan_id: Path<i64>,
    body: Json<NewShoppingListItem>,
) -> impl Responder {
    match store.add_shopping_list_item(plan_id.into_inner(), body.into_inner()) {
        Ok(list) => HttpResponse::Created().json(list),
        Err(error) => error_response(error),
    }
}

#[put("/plans/{plan_id}/shopping-list/items/{item_id}")]
async fn update_shopping_list_item(
    store: Data<MealPlannerStore>,
    path: Path<(i64, i64)>,
    body: Json<ShoppingListItemUpdate>,
) -> impl Responder {
    let (plan_id, item_id) = path.into_inner();
    match store.update_shopping_list_item(plan_id, item_id, body.into_inner()) {
        Ok(list) => HttpResponse::Ok().json(list),
        Err(error) => error_response(error),
    }
}

#[delete("/plans/{plan_id}/shopping-list/items/{item_id}")]
async fn remove_shopping_list_item(
    store: Data<MealPlannerStore>,
    path: Path<(i64, i64)>,
) -> impl Responder {
    let (plan_id, item_id) = path.into_inner();
    match store.remove_shopping_list_item(plan_id, item_id) {
        Ok(list) => HttpResponse::Ok().json(list),
        Err(error) => error_response(error),
    }
}

pub fn meal_planner_api() -> Scope {
    web::scope("/meal-planner")
        .service(list_members)
        .service(create_member)
        .service(list_recipes)
        .service(create_recipe)
        .service(get_recipe)
        .service(update_recipe)
        .service(delete_recipe)
        .service(get_weekly_plan)
        .service(list_weekly_plan_revisions)
        .service(create_weekly_plan)
        .service(update_weekly_plan)
        .service(confirm_weekly_plan)
        .service(revise_weekly_plan)
        .service(get_shopping_list)
        .service(generate_shopping_list)
        .service(add_shopping_list_item)
        .service(update_shopping_list_item)
        .service(remove_shopping_list_item)
}
