//! Authenticated HTTP API for the household recipe catalogue.
use actix_web::{
    HttpRequest, HttpResponse, Responder, Scope, delete, get, post, put,
    web::{self, Data, Json, Path, Query},
};
use agent_service::FamilyService;
use agent_storage::{FamilyId, FileUserIdentityStore, UserId, UserIdentityStore};
use chrono::NaiveDate;
use meal_planner::{
    MealPlannerApplication, MealPlannerError, MealPlannerStore, NewShoppingListItem, RecipeDraft,
    ShoppingListItemUpdate, WeeklyPlanDraft,
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
    match sync_family_roster(user, &family_id.to_string(), &identities, &families, &store).await {
        Ok(()) => match store.list_family_members_for_family(&family_id.to_string()) {
            Ok(members) => HttpResponse::Ok().json(
                members
                    .into_iter()
                    .filter(|member| member.active)
                    .collect::<Vec<_>>(),
            ),
            Err(error) => error_response(error),
        },
        Err(response) => response,
    }
}

async fn sync_family_roster(
    user: UserId,
    family_id: &str,
    identities: &FileUserIdentityStore,
    families: &FamilyService,
    store: &MealPlannerStore,
) -> Result<(), HttpResponse> {
    let id = FamilyId::from_str(family_id)
        .map_err(|_| HttpResponse::BadRequest().body("invalid family id"))?;
    let (_, memberships) = families
        .get_family(user, id)
        .await
        .map_err(|_| HttpResponse::NotFound().finish())?;
    let mut members = Vec::with_capacity(memberships.len());
    for member in memberships {
        let profile = identities
            .get_user(member.user_id.clone())
            .map_err(|_| HttpResponse::InternalServerError().finish())?;
        let name = profile
            .and_then(|user| user.display_name)
            .unwrap_or_else(|| member.user_id.to_string());
        members.push((member.user_id.to_string(), name));
    }
    store
        .sync_family_members(family_id, &members)
        .map_err(error_response)?;
    Ok(())
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

#[get("/context/{week_start}")]
async fn get_planning_context(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<Arc<FamilyService>>,
    store: Data<MealPlannerStore>,
    week_start: Path<NaiveDate>,
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
    if let Err(response) =
        sync_family_roster(user.clone(), &family_id, &identities, &families, &store).await
    {
        return response;
    }
    let context = MealPlannerApplication::new(store.get_ref().clone()).planning_context(
        &user.to_string(),
        &[family_id],
        week_start.into_inner(),
    );
    match context {
        Ok(context) => HttpResponse::Ok().json(context),
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
    match store.save_weekly_plan_for_family(&family_id, week_start.into_inner(), body.into_inner())
    {
        Ok(plan) => HttpResponse::Ok().json(plan),
        Err(error) => error_response(error),
    }
}

#[delete("/plans/{week_start}")]
async fn delete_weekly_plan(
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
        Ok(v) => v,
        Err(r) => return r,
    };
    match store.delete_weekly_plan_for_family(&family_id, week_start.into_inner()) {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(e) => error_response(e),
    }
}
#[get("/shopping-list")]
async fn get_shopping_list(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<Arc<FamilyService>>,
    store: Data<MealPlannerStore>,
    query: Query<ShoppingRangeQuery>,
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
        Ok(v) => v,
        Err(r) => return r,
    };
    let (Some(start), Some(end)) = (query.start_date, query.end_date) else {
        return HttpResponse::BadRequest().body("start_date and end_date are required");
    };
    match store.get_shopping_list_for_range(&family_id, start, end) {
        Ok(Some(v)) => HttpResponse::Ok().json(v),
        Ok(None) => HttpResponse::NotFound().finish(),
        Err(e) => error_response(e),
    }
}
#[post("/shopping-list/generate")]
async fn generate_shopping_list(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<Arc<FamilyService>>,
    store: Data<MealPlannerStore>,
    query: Query<ShoppingRangeQuery>,
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
        Ok(v) => v,
        Err(r) => return r,
    };
    let (Some(start), Some(end)) = (query.start_date, query.end_date) else {
        return HttpResponse::BadRequest().body("start_date and end_date are required");
    };
    match store.generate_shopping_list_for_range(&family_id, start, end) {
        Ok(v) => HttpResponse::Ok().json(v),
        Err(e) => error_response(e),
    }
}
#[post("/shopping-list/{list_id}/items")]
async fn add_shopping_list_item(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<Arc<FamilyService>>,
    store: Data<MealPlannerStore>,
    list_id: Path<i64>,
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
        Ok(v) => v,
        Err(r) => return r,
    };
    let id = list_id.into_inner();
    if !store
        .shopping_list_belongs_to_family(id, &family_id)
        .unwrap_or(false)
    {
        return HttpResponse::NotFound().finish();
    }
    match store.add_shopping_list_item(id, body.into_inner()) {
        Ok(v) => HttpResponse::Created().json(v),
        Err(e) => error_response(e),
    }
}
#[put("/shopping-list/{list_id}/items/{item_id}")]
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
        Ok(v) => v,
        Err(r) => return r,
    };
    let (list_id, item_id) = path.into_inner();
    if !store
        .shopping_list_belongs_to_family(list_id, &family_id)
        .unwrap_or(false)
    {
        return HttpResponse::NotFound().finish();
    }
    match store.update_shopping_list_item(list_id, item_id, body.into_inner()) {
        Ok(v) => HttpResponse::Ok().json(v),
        Err(e) => error_response(e),
    }
}
#[post("/shopping-list/{list_id}/items/{item_id}/restore")]
async fn restore_shopping_list_item(
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
        Ok(v) => v,
        Err(r) => return r,
    };
    let (list_id, item_id) = path.into_inner();
    if !store
        .shopping_list_belongs_to_family(list_id, &family_id)
        .unwrap_or(false)
    {
        return HttpResponse::NotFound().finish();
    }
    match store.restore_shopping_list_item(list_id, item_id) {
        Ok(v) => HttpResponse::Ok().json(v),
        Err(e) => error_response(e),
    }
}
#[delete("/shopping-list/{list_id}/items/{item_id}")]
async fn remove_shopping_list_item(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<FamilyService>,
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
        Ok(v) => v,
        Err(r) => return r,
    };
    let (list_id, item_id) = path.into_inner();
    if !store
        .shopping_list_belongs_to_family(list_id, &family_id)
        .unwrap_or(false)
    {
        return HttpResponse::NotFound().finish();
    }
    match store.remove_shopping_list_item(list_id, item_id) {
        Ok(v) => HttpResponse::Ok().json(v),
        Err(e) => error_response(e),
    }
}
#[derive(Deserialize)]
struct ShoppingRangeQuery {
    family_id: Option<String>,
    start_date: Option<NaiveDate>,
    end_date: Option<NaiveDate>,
}

pub fn meal_planner_api() -> Scope {
    web::scope("/meal-planner")
        .service(list_members)
        .service(get_planning_context)
        .service(list_recipes)
        .service(create_recipe)
        .service(get_recipe)
        .service(update_recipe)
        .service(delete_recipe)
        .service(get_weekly_plan)
        .service(create_weekly_plan)
        .service(update_weekly_plan)
        .service(delete_weekly_plan)
        .service(get_shopping_list)
        .service(generate_shopping_list)
        .service(add_shopping_list_item)
        .service(update_shopping_list_item)
        .service(restore_shopping_list_item)
        .service(remove_shopping_list_item)
}
