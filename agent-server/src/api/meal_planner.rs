//! Authenticated HTTP API for the household recipe catalogue.
use actix_web::{
    HttpResponse, Responder, Scope, delete, get, post, put,
    web::{self, Data, Json, Path, Query},
};
use chrono::NaiveDate;
use meal_planner::{MealPlannerError, MealPlannerStore, RecipeDraft, WeeklyPlanDraft};
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

#[get("/recipes")]
async fn list_recipes(store: Data<MealPlannerStore>, query: Query<SearchQuery>) -> impl Responder {
    match store.list_recipes(query.q.as_deref()) {
        Ok(recipes) => HttpResponse::Ok().json(recipes),
        Err(error) => error_response(error),
    }
}

#[post("/recipes")]
async fn create_recipe(store: Data<MealPlannerStore>, body: Json<RecipeDraft>) -> impl Responder {
    match store.create_recipe(body.into_inner()) {
        Ok(recipe) => HttpResponse::Created().json(recipe),
        Err(error) => error_response(error),
    }
}

#[get("/recipes/{id}")]
async fn get_recipe(store: Data<MealPlannerStore>, id: Path<i64>) -> impl Responder {
    match store.get_recipe(id.into_inner()) {
        Ok(recipe) => HttpResponse::Ok().json(recipe),
        Err(error) => error_response(error),
    }
}

#[put("/recipes/{id}")]
async fn update_recipe(
    store: Data<MealPlannerStore>,
    id: Path<i64>,
    body: Json<RecipeDraft>,
) -> impl Responder {
    match store.update_recipe(id.into_inner(), body.into_inner()) {
        Ok(recipe) => HttpResponse::Ok().json(recipe),
        Err(error) => error_response(error),
    }
}

#[delete("/recipes/{id}")]
async fn delete_recipe(store: Data<MealPlannerStore>, id: Path<i64>) -> impl Responder {
    match store.delete_recipe(id.into_inner()) {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(error) => error_response(error),
    }
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

pub fn meal_planner_api() -> Scope {
    web::scope("/meal-planner")
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
}
