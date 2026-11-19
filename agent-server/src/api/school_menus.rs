//! HTTP API for manual school menu management and provider-neutral imports.
use actix_web::{
    HttpRequest, HttpResponse, Responder, Scope, delete, get, post, put,
    web::{self, Data, Json, Path, Query},
};
use agent_service::FamilyService;
use agent_storage::{FileUserIdentityStore, UserId};
use chrono::NaiveDate;
use meal_planner::{MealPlannerError, MealPlannerStore, SchoolMenuDraft};
use serde::Deserialize;
use std::sync::Arc;
#[derive(Deserialize)]
struct ImportRequest {
    source: String,
    entries: Vec<SchoolMenuDraft>,
}
fn error_response(e: MealPlannerError) -> HttpResponse {
    match e {
        MealPlannerError::NotFound => HttpResponse::NotFound().finish(),
        MealPlannerError::InvalidInput(m) => HttpResponse::BadRequest().body(m),
        MealPlannerError::Conflict(m) => HttpResponse::Conflict().body(m),
        _ => HttpResponse::InternalServerError().finish(),
    }
}
#[derive(Deserialize)]
struct FamilyQuery {
    family_id: Option<String>,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
}

async fn authorize_family(
    request: &HttpRequest,
    auth: &crate::auth::TelegramAuth,
    identities: &FileUserIdentityStore,
    families: &FamilyService,
    requested: Option<&str>,
) -> Result<String, HttpResponse> {
    let user: UserId = super::meal_planner::authenticated_user(request, auth, identities)?;
    let ids = super::meal_planner::family_ids(user, families).await?;
    let id = requested.ok_or_else(|| HttpResponse::BadRequest().body("family_id is required"))?;
    super::meal_planner::validate_family(Some(id), &ids)?;
    Ok(id.to_owned())
}

async fn list_handler(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<Arc<FamilyService>>,
    store: Data<MealPlannerStore>,
    q: Query<FamilyQuery>,
) -> impl Responder {
    let family_id = match authorize_family(
        &request,
        &auth,
        &identities,
        &families,
        q.family_id.as_deref(),
    )
    .await
    {
        Ok(id) => id,
        Err(response) => return response,
    };
    let (Some(from), Some(to)) = (q.from, q.to) else {
        return HttpResponse::BadRequest().finish();
    };
    match store.list_school_menus_for_family(&family_id, from, to) {
        Ok(v) => HttpResponse::Ok().json(v),
        Err(e) => error_response(e),
    }
}

#[get("/{date}")]
async fn get_date(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<Arc<FamilyService>>,
    store: Data<MealPlannerStore>,
    date: Path<NaiveDate>,
    q: Query<FamilyQuery>,
) -> impl Responder {
    let family_id = match authorize_family(
        &request,
        &auth,
        &identities,
        &families,
        q.family_id.as_deref(),
    )
    .await
    {
        Ok(id) => id,
        Err(response) => return response,
    };
    match store.get_school_menu_for_family(&family_id, date.into_inner()) {
        Ok(Some(v)) => HttpResponse::Ok().json(v),
        Ok(None) => HttpResponse::NotFound().finish(),
        Err(e) => error_response(e),
    }
}

#[put("/{date}")]
async fn save(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<Arc<FamilyService>>,
    store: Data<MealPlannerStore>,
    date: Path<NaiveDate>,
    q: Query<FamilyQuery>,
    body: Json<SchoolMenuDraft>,
) -> impl Responder {
    let family_id = match authorize_family(
        &request,
        &auth,
        &identities,
        &families,
        q.family_id.as_deref(),
    )
    .await
    {
        Ok(id) => id,
        Err(response) => return response,
    };
    let date = date.into_inner();
    let draft = body.into_inner();
    if draft.date != date {
        return HttpResponse::BadRequest().body("path date and menu date must match");
    }
    match store.save_school_menu_for_family(&family_id, draft) {
        Ok(v) => HttpResponse::Ok().json(v),
        Err(e) => error_response(e),
    }
}

#[delete("/{date}")]
async fn remove(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<Arc<FamilyService>>,
    store: Data<MealPlannerStore>,
    date: Path<NaiveDate>,
    q: Query<FamilyQuery>,
) -> impl Responder {
    let family_id = match authorize_family(
        &request,
        &auth,
        &identities,
        &families,
        q.family_id.as_deref(),
    )
    .await
    {
        Ok(id) => id,
        Err(response) => return response,
    };
    match store.delete_school_menu_for_family(&family_id, date.into_inner()) {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(e) => error_response(e),
    }
}

#[post("/import")]
async fn import(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    families: Data<Arc<FamilyService>>,
    store: Data<MealPlannerStore>,
    q: Query<FamilyQuery>,
    body: Json<ImportRequest>,
) -> impl Responder {
    let family_id = match authorize_family(
        &request,
        &auth,
        &identities,
        &families,
        q.family_id.as_deref(),
    )
    .await
    {
        Ok(id) => id,
        Err(response) => return response,
    };
    let ImportRequest { source, entries } = body.into_inner();
    match store.import_school_menus_for_family(&family_id, &source, entries) {
        Ok(v) => HttpResponse::Ok().json(v),
        Err(e) => error_response(e),
    }
}

pub fn school_menus_api() -> Scope {
    web::scope("/meal-planner/school-menus")
        // The browser may request the collection URL with or without a
        // trailing slash. Register both forms explicitly.
        .route("", web::get().to(list_handler))
        .route("/", web::get().to(list_handler))
        .service(get_date)
        .service(save)
        .service(remove)
        .service(import)
}
#[cfg(test)]
mod tests {
    use super::*;
    use meal_planner::{SchoolMenuItem, SchoolMenuStatus};

    #[test]
    fn school_menus_are_isolated_by_family() {
        let store = MealPlannerStore::open_in_memory().unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
        let draft = SchoolMenuDraft {
            date,
            status: SchoolMenuStatus::Known,
            items: vec![SchoolMenuItem {
                name: "Lentils".into(),
                notes: None,
                allergens: None,
                dietary_alternatives: vec![],
            }],
            notes: None,
        };
        store
            .save_school_menu_for_family("family-a", draft.clone())
            .unwrap();
        assert_eq!(
            store
                .list_school_menus_for_family("family-a", date, date)
                .unwrap()
                .len(),
            1
        );
        assert!(
            store
                .list_school_menus_for_family("family-b", date, date)
                .unwrap()
                .is_empty()
        );
    }
}
