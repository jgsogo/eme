//! Authenticated HTTP API for the household recipe catalogue.
use actix_web::{
    HttpResponse, Responder, Scope, delete, get, post, put,
    web::{self, Data, Json, Path, Query},
};
use meal_planner::{MealPlannerError, MealPlannerStore, RecipeDraft};
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

pub fn meal_planner_api() -> Scope {
    web::scope("/meal-planner")
        .service(list_recipes)
        .service(create_recipe)
        .service(get_recipe)
        .service(update_recipe)
        .service(delete_recipe)
}
