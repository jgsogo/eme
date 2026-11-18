//! HTTP API for manual school menu management and provider-neutral imports.
use actix_web::{
    HttpResponse, Responder, Scope, delete, get, post, put,
    web::{self, Data, Json, Path, Query},
};
use chrono::NaiveDate;
use meal_planner::{MealPlannerError, MealPlannerStore, SchoolMenuDraft};
use serde::Deserialize;
#[derive(Deserialize)]
struct RangeQuery {
    from: NaiveDate,
    to: NaiveDate,
}
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
#[get("")]
async fn list(store: Data<MealPlannerStore>, q: Query<RangeQuery>) -> impl Responder {
    match store.list_school_menus(q.from, q.to) {
        Ok(v) => HttpResponse::Ok().json(v),
        Err(e) => error_response(e),
    }
}
#[get("/{date}")]
async fn get_date(store: Data<MealPlannerStore>, date: Path<NaiveDate>) -> impl Responder {
    match store.get_school_menu(date.into_inner()) {
        Ok(Some(v)) => HttpResponse::Ok().json(v),
        Ok(None) => HttpResponse::NotFound().finish(),
        Err(e) => error_response(e),
    }
}
#[put("/{date}")]
async fn save(
    store: Data<MealPlannerStore>,
    date: Path<NaiveDate>,
    body: Json<SchoolMenuDraft>,
) -> impl Responder {
    let date = date.into_inner();
    let d = body.into_inner();
    if d.date != date {
        return HttpResponse::BadRequest().body("path date and menu date must match");
    }
    match store.save_school_menu(d) {
        Ok(v) => HttpResponse::Ok().json(v),
        Err(e) => error_response(e),
    }
}
#[delete("/{date}")]
async fn remove(store: Data<MealPlannerStore>, date: Path<NaiveDate>) -> impl Responder {
    match store.delete_school_menu(date.into_inner()) {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(e) => error_response(e),
    }
}
#[post("/import")]
async fn import(store: Data<MealPlannerStore>, body: Json<ImportRequest>) -> impl Responder {
    let ImportRequest { source, entries } = body.into_inner();
    match store.import_school_menus(&source, entries) {
        Ok(v) => HttpResponse::Ok().json(v),
        Err(e) => error_response(e),
    }
}
pub fn school_menus_api() -> Scope {
    web::scope("/meal-planner/school-menus")
        .service(list)
        .service(get_date)
        .service(save)
        .service(remove)
        .service(import)
}
#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, test};
    use meal_planner::{SchoolMenuEntry, SchoolMenuItem, SchoolMenuStatus};
    fn draft() -> SchoolMenuDraft {
        SchoolMenuDraft {
            date: NaiveDate::from_ymd_opt(2026, 11, 16).unwrap(),
            status: SchoolMenuStatus::Known,
            items: vec![SchoolMenuItem {
                name: "Lentils".into(),
                notes: None,
                allergens: None,
                dietary_alternatives: vec![],
            }],
            notes: None,
        }
    }
    #[actix_web::test]
    async fn manual_menu_can_be_saved_and_retrieved() {
        let s = MealPlannerStore::open_in_memory().unwrap();
        let app = test::init_service(
            App::new()
                .app_data(Data::new(s))
                .service(school_menus_api()),
        )
        .await;
        let req = test::TestRequest::put()
            .uri("/meal-planner/school-menus/2026-11-16")
            .set_json(draft())
            .to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            actix_web::http::StatusCode::OK
        );
        let req = test::TestRequest::get()
            .uri("/meal-planner/school-menus/2026-11-16")
            .to_request();
        let v: SchoolMenuEntry = test::call_and_read_body_json(&app, req).await;
        assert_eq!(v.draft.items[0].name, "Lentils")
    }
    #[actix_web::test]
    async fn path_date_mismatch_is_rejected() {
        let s = MealPlannerStore::open_in_memory().unwrap();
        let app = test::init_service(
            App::new()
                .app_data(Data::new(s))
                .service(school_menus_api()),
        )
        .await;
        let req = test::TestRequest::put()
            .uri("/meal-planner/school-menus/2026-11-17")
            .set_json(draft())
            .to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            actix_web::http::StatusCode::BAD_REQUEST
        )
    }
}
