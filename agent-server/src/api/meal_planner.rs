//! Authenticated HTTP API for the household recipe catalogue.
use actix_web::{
    HttpRequest, HttpResponse, Responder, Scope, delete, get, post, put,
    web::{self, Data, Json, Path, Query},
};
use agent_service::FamilyService;
use agent_storage::{FamilyId, FileUserIdentityStore, UserId, UserIdentityStore};
use chrono::NaiveDate;
use meal_planner::{
    MealPlannerError, MealPlannerStore, NewShoppingListItem, RecipeDraft, ShoppingListItemUpdate,
    WeeklyPlanDraft,
};
use serde::Deserialize;
use std::{str::FromStr, sync::Arc};

#[derive(Deserialize)]
struct SearchQuery {
    q: Option<String>,
    family_id: Option<String>,
}

fn error_response(error: MealPlannerError) -> HttpResponse {
    match error {
        MealPlannerError::NotFound => HttpResponse::NotFound().finish(),
        MealPlannerError::InvalidInput(message) => HttpResponse::BadRequest().body(message),
        MealPlannerError::Conflict(message) => HttpResponse::Conflict().body(message),
        _ => HttpResponse::InternalServerError().finish(),
    }
}

#[get("/members")]
async fn list_members(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<Arc<FamilyService>>,
    store: Data<MealPlannerStore>,
    query: Query<SearchQuery>,
) -> impl Responder {
    let user = match authenticated_user(&request, &auth, &identities) {
        Ok(user) => user,
        Err(response) => return response,
    };
    let family_id = match query.family_id.as_deref() {
        Some(value) => match FamilyId::from_str(value) {
            Ok(id) => id,
            Err(_) => return HttpResponse::BadRequest().body("invalid family id"),
        },
        None => return HttpResponse::BadRequest().body("family_id is required"),
    };
    let (_, memberships) = match families.get_family(user, family_id).await {
        Ok(value) => value,
        Err(_) => return HttpResponse::NotFound().finish(),
    };
    let mut members = Vec::with_capacity(memberships.len());
    for member in memberships {
        let profile = match identities.get_user(member.user_id.clone()) {
            Ok(profile) => profile,
            Err(_) => return HttpResponse::InternalServerError().finish(),
        };
        let name = profile
            .and_then(|user| user.display_name)
            .unwrap_or_else(|| member.user_id.to_string());
        members.push((member.user_id.to_string(), name));
    }
    match store.sync_family_members(&family_id.to_string(), &members) {
        Ok(members) => HttpResponse::Ok().json(
            members
                .into_iter()
                .filter(|member| member.active)
                .collect::<Vec<_>>(),
        ),
        Err(error) => error_response(error),
    }
}

pub(crate) fn authenticated_user(
    request: &HttpRequest,
    auth: &crate::auth::TelegramAuth,
    identities: &FileUserIdentityStore,
) -> Result<UserId, HttpResponse> {
    let owner = auth
        .owner_for_request(request)
        .ok_or_else(|| HttpResponse::Unauthorized().finish())?;
    let subject = owner
        .strip_prefix("telegram:")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| HttpResponse::Unauthorized().finish())?;
    identities
        .find_identity("telegram", subject)
        .map_err(|_| HttpResponse::InternalServerError().finish())?
        .map(|identity| identity.user_id)
        .ok_or_else(|| HttpResponse::Unauthorized().finish())
}

pub(crate) async fn family_ids(
    user: UserId,
    families: &FamilyService,
) -> Result<Vec<String>, HttpResponse> {
    families
        .list_families(user)
        .await
        .map(|families| {
            families
                .into_iter()
                .map(|family| family.id.to_string())
                .collect()
        })
        .map_err(|_| HttpResponse::InternalServerError().finish())
}

pub(crate) fn validate_family(id: Option<&str>, available: &[String]) -> Result<(), HttpResponse> {
    if let Some(id) = id {
        FamilyId::from_str(id).map_err(|_| HttpResponse::BadRequest().body("invalid family id"))?;
        if !available.iter().any(|family| family == id) {
            return Err(HttpResponse::NotFound().finish());
        }
    }
    Ok(())
}

async fn selected_family(
    request: &HttpRequest,
    auth: &crate::auth::TelegramAuth,
    identities: &FileUserIdentityStore,
    families: &FamilyService,
    requested: Option<&str>,
) -> Result<String, HttpResponse> {
    let user = authenticated_user(request, auth, identities)?;
    let available = family_ids(user, families).await?;
    let id = requested.ok_or_else(|| HttpResponse::BadRequest().body("family_id is required"))?;
    validate_family(Some(id), &available)?;
    Ok(id.to_owned())
}

#[get("/recipes")]
async fn list_recipes(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<Arc<FamilyService>>,
    store: Data<MealPlannerStore>,
    query: Query<SearchQuery>,
) -> impl Responder {
    let user = match authenticated_user(&request, &auth, &identities) {
        Ok(user) => user,
        Err(response) => return response,
    };
    let ids = match family_ids(user, &families).await {
        Ok(ids) => ids,
        Err(response) => return response,
    };

    let family_id = match query.family_id.as_deref() {
        Some(id) => id,
        None => return HttpResponse::BadRequest().body("family_id is required"),
    };
    if let Err(response) = validate_family(Some(family_id), &ids) {
        return response;
    }
    match store.list_recipes(
        &user.to_string(),
        &[family_id.to_owned()],
        query.q.as_deref(),
    ) {
        Ok(recipes) => HttpResponse::Ok().json(recipes),
        Err(error) => error_response(error),
    }
}

#[post("/recipes")]
async fn create_recipe(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<Arc<FamilyService>>,
    store: Data<MealPlannerStore>,
    query: Query<SearchQuery>,
    body: Json<RecipeDraft>,
) -> impl Responder {
    let user = match authenticated_user(&request, &auth, &identities) {
        Ok(user) => user,
        Err(response) => return response,
    };
    let family_id = match selected_family(
        &request,
        &auth,
        &identities,
        &families,
        query.family_id.as_deref(),
    )
    .await
    {
        Ok(id) => id,
        Err(response) => return response,
    };
    if body
        .family_id
        .as_deref()
        .is_some_and(|shared| shared != family_id.as_str())
    {
        return HttpResponse::BadRequest()
            .body("recipes can only be shared with the selected family");
    }

    match store.create_recipe(&user.to_string(), body.into_inner()) {
        Ok(recipe) => HttpResponse::Created().json(recipe),
        Err(error) => error_response(error),
    }
}

#[get("/recipes/{id}")]
async fn get_recipe(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<Arc<FamilyService>>,
    store: Data<MealPlannerStore>,
    id: Path<i64>,
    query: Query<SearchQuery>,
) -> impl Responder {
    let user = match authenticated_user(&request, &auth, &identities) {
        Ok(user) => user,
        Err(response) => return response,
    };
    let ids = match family_ids(user, &families).await {
        Ok(ids) => ids,
        Err(response) => return response,
    };

    let family_id = match query.family_id.as_deref() {
        Some(id) => id,
        None => return HttpResponse::BadRequest().body("family_id is required"),
    };
    if let Err(response) = validate_family(Some(family_id), &ids) {
        return response;
    }
    match store.get_recipe(id.into_inner(), &user.to_string(), &[family_id.to_owned()]) {
        Ok(recipe) => HttpResponse::Ok().json(recipe),
        Err(error) => error_response(error),
    }
}

#[put("/recipes/{id}")]
async fn update_recipe(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<Arc<FamilyService>>,
    store: Data<MealPlannerStore>,
    id: Path<i64>,
    query: Query<SearchQuery>,
    body: Json<RecipeDraft>,
) -> impl Responder {
    let user = match authenticated_user(&request, &auth, &identities) {
        Ok(user) => user,
        Err(response) => return response,
    };
    let family_id = match selected_family(
        &request,
        &auth,
        &identities,
        &families,
        query.family_id.as_deref(),
    )
    .await
    {
        Ok(id) => id,
        Err(response) => return response,
    };
    if body
        .family_id
        .as_deref()
        .is_some_and(|shared| shared != family_id)
    {
        return HttpResponse::BadRequest()
            .body("recipes can only be shared with the selected family");
    }

    match store.update_recipe_for_family(
        id.into_inner(),
        &user.to_string(),
        &family_id,
        body.into_inner(),
    ) {
        Ok(recipe) => HttpResponse::Ok().json(recipe),
        Err(error) => error_response(error),
    }
}

#[delete("/recipes/{id}")]
async fn delete_recipe(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<Arc<FamilyService>>,
    store: Data<MealPlannerStore>,
    id: Path<i64>,
    query: Query<SearchQuery>,
) -> impl Responder {
    let user = match authenticated_user(&request, &auth, &identities) {
        Ok(user) => user,
        Err(response) => return response,
    };
    let family_id = match selected_family(
        &request,
        &auth,
        &identities,
        &families,
        query.family_id.as_deref(),
    )
    .await
    {
        Ok(id) => id,
        Err(response) => return response,
    };
    match store.delete_recipe_for_family(id.into_inner(), &user.to_string(), &family_id) {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(error) => error_response(error),
    }
}

#[get("/plans/{week_start}")]
async fn get_weekly_plan(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<Arc<FamilyService>>,
    store: Data<MealPlannerStore>,
    week_start: Path<NaiveDate>,
    query: Query<SearchQuery>,
) -> impl Responder {
    let family_id = match selected_family(
        &request,
        &auth,
        &identities,
        &families,
        query.family_id.as_deref(),
    )
    .await
    {
        Ok(id) => id,
        Err(response) => return response,
    };
    match store.get_weekly_plan_for_family(&family_id, week_start.into_inner()) {
        Ok(Some(plan)) => HttpResponse::Ok().json(Some(plan)),
        // An unplanned week is a normal empty state for the monthly calendar.
        Ok(None) => HttpResponse::Ok().json(None::<()>),
        Err(error) => error_response(error),
    }
}
#[get("/plans/{week_start}/revisions")]
async fn list_weekly_plan_revisions(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<Arc<FamilyService>>,
    store: Data<MealPlannerStore>,
    week_start: Path<NaiveDate>,
    query: Query<SearchQuery>,
) -> impl Responder {
    let family_id = match selected_family(
        &request,
        &auth,
        &identities,
        &families,
        query.family_id.as_deref(),
    )
    .await
    {
        Ok(id) => id,
        Err(response) => return response,
    };
    match store.list_weekly_plan_revisions_for_family(&family_id, week_start.into_inner()) {
        Ok(plans) => HttpResponse::Ok().json(plans),
        Err(error) => error_response(error),
    }
}
#[post("/plans/{week_start}")]
async fn create_weekly_plan(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<Arc<FamilyService>>,
    store: Data<MealPlannerStore>,
    week_start: Path<NaiveDate>,
    query: Query<SearchQuery>,
    body: Json<WeeklyPlanDraft>,
) -> impl Responder {
    let family_id = match selected_family(
        &request,
        &auth,
        &identities,
        &families,
        query.family_id.as_deref(),
    )
    .await
    {
        Ok(id) => id,
        Err(response) => return response,
    };
    match store.create_weekly_plan_for_family(
        &family_id,
        week_start.into_inner(),
        body.into_inner(),
    ) {
        Ok(plan) => HttpResponse::Created().json(plan),
        Err(error) => error_response(error),
    }
}
#[put("/plans/{week_start}")]
async fn update_weekly_plan(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<Arc<FamilyService>>,
    store: Data<MealPlannerStore>,
    week_start: Path<NaiveDate>,
    query: Query<SearchQuery>,
    body: Json<WeeklyPlanDraft>,
) -> impl Responder {
    let family_id = match selected_family(
        &request,
        &auth,
        &identities,
        &families,
        query.family_id.as_deref(),
    )
    .await
    {
        Ok(id) => id,
        Err(response) => return response,
    };
    match store.update_weekly_plan_for_family(
        &family_id,
        week_start.into_inner(),
        body.into_inner(),
    ) {
        Ok(plan) => HttpResponse::Ok().json(plan),
        Err(error) => error_response(error),
    }
}
#[post("/plans/{week_start}/confirm")]
async fn confirm_weekly_plan(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<Arc<FamilyService>>,
    store: Data<MealPlannerStore>,
    week_start: Path<NaiveDate>,
    query: Query<SearchQuery>,
) -> impl Responder {
    let family_id = match selected_family(
        &request,
        &auth,
        &identities,
        &families,
        query.family_id.as_deref(),
    )
    .await
    {
        Ok(id) => id,
        Err(response) => return response,
    };
    match store.confirm_weekly_plan_for_family(&family_id, week_start.into_inner()) {
        Ok(plan) => HttpResponse::Ok().json(plan),
        Err(error) => error_response(error),
    }
}
#[post("/plans/{week_start}/revise")]
async fn revise_weekly_plan(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<Arc<FamilyService>>,
    store: Data<MealPlannerStore>,
    week_start: Path<NaiveDate>,
    query: Query<SearchQuery>,
) -> impl Responder {
    let family_id = match selected_family(
        &request,
        &auth,
        &identities,
        &families,
        query.family_id.as_deref(),
    )
    .await
    {
        Ok(id) => id,
        Err(response) => return response,
    };
    match store.revise_weekly_plan_for_family(&family_id, week_start.into_inner()) {
        Ok(plan) => HttpResponse::Created().json(plan),
        Err(error) => error_response(error),
    }
}
#[get("/plans/{plan_id}/shopping-list")]
async fn get_shopping_list(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<Arc<FamilyService>>,
    store: Data<MealPlannerStore>,
    plan_id: Path<i64>,
    query: Query<SearchQuery>,
) -> impl Responder {
    let family_id = match selected_family(
        &request,
        &auth,
        &identities,
        &families,
        query.family_id.as_deref(),
    )
    .await
    {
        Ok(id) => id,
        Err(response) => return response,
    };
    let id = plan_id.into_inner();
    if !store
        .plan_belongs_to_family(id, &family_id)
        .unwrap_or(false)
    {
        return HttpResponse::NotFound().finish();
    }
    match store.get_shopping_list(id) {
        Ok(Some(list)) => HttpResponse::Ok().json(list),
        Ok(None) => HttpResponse::NotFound().finish(),
        Err(error) => error_response(error),
    }
}
#[post("/plans/{plan_id}/shopping-list/generate")]
async fn generate_shopping_list(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<Arc<FamilyService>>,
    store: Data<MealPlannerStore>,
    plan_id: Path<i64>,
    query: Query<SearchQuery>,
) -> impl Responder {
    let family_id = match selected_family(
        &request,
        &auth,
        &identities,
        &families,
        query.family_id.as_deref(),
    )
    .await
    {
        Ok(id) => id,
        Err(response) => return response,
    };
    let id = plan_id.into_inner();
    if !store
        .plan_belongs_to_family(id, &family_id)
        .unwrap_or(false)
    {
        return HttpResponse::NotFound().finish();
    }
    match store.generate_shopping_list(id) {
        Ok(list) => HttpResponse::Ok().json(list),
        Err(error) => error_response(error),
    }
}
#[post("/plans/{plan_id}/shopping-list/items")]
async fn add_shopping_list_item(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<Arc<FamilyService>>,
    store: Data<MealPlannerStore>,
    plan_id: Path<i64>,
    query: Query<SearchQuery>,
    body: Json<NewShoppingListItem>,
) -> impl Responder {
    let family_id = match selected_family(
        &request,
        &auth,
        &identities,
        &families,
        query.family_id.as_deref(),
    )
    .await
    {
        Ok(id) => id,
        Err(response) => return response,
    };
    let id = plan_id.into_inner();
    if !store
        .plan_belongs_to_family(id, &family_id)
        .unwrap_or(false)
    {
        return HttpResponse::NotFound().finish();
    }
    match store.add_shopping_list_item(id, body.into_inner()) {
        Ok(list) => HttpResponse::Created().json(list),
        Err(error) => error_response(error),
    }
}
#[put("/plans/{plan_id}/shopping-list/items/{item_id}")]
async fn update_shopping_list_item(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<Arc<FamilyService>>,
    store: Data<MealPlannerStore>,
    path: Path<(i64, i64)>,
    query: Query<SearchQuery>,
    body: Json<ShoppingListItemUpdate>,
) -> impl Responder {
    let family_id = match selected_family(
        &request,
        &auth,
        &identities,
        &families,
        query.family_id.as_deref(),
    )
    .await
    {
        Ok(id) => id,
        Err(response) => return response,
    };
    let (plan_id, item_id) = path.into_inner();
    if !store
        .plan_belongs_to_family(plan_id, &family_id)
        .unwrap_or(false)
    {
        return HttpResponse::NotFound().finish();
    }
    match store.update_shopping_list_item(plan_id, item_id, body.into_inner()) {
        Ok(list) => HttpResponse::Ok().json(list),
        Err(error) => error_response(error),
    }
}
#[delete("/plans/{plan_id}/shopping-list/items/{item_id}")]
async fn remove_shopping_list_item(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<Arc<FamilyService>>,
    store: Data<MealPlannerStore>,
    path: Path<(i64, i64)>,
    query: Query<SearchQuery>,
) -> impl Responder {
    let family_id = match selected_family(
        &request,
        &auth,
        &identities,
        &families,
        query.family_id.as_deref(),
    )
    .await
    {
        Ok(id) => id,
        Err(response) => return response,
    };
    let (plan_id, item_id) = path.into_inner();
    if !store
        .plan_belongs_to_family(plan_id, &family_id)
        .unwrap_or(false)
    {
        return HttpResponse::NotFound().finish();
    }
    match store.remove_shopping_list_item(plan_id, item_id) {
        Ok(list) => HttpResponse::Ok().json(list),
        Err(error) => error_response(error),
    }
}

pub fn meal_planner_api() -> Scope {
    web::scope("/meal-planner")
        .service(list_members)
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
