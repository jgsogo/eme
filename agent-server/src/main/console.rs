//! Serves the independent web console pages and their shared assets.

use actix_web::{HttpResponse, Responder, get, http::header};

#[get("/")]
pub(super) async fn console_root() -> impl Responder {
    HttpResponse::Found()
        .append_header((header::LOCATION, "/console/conversations"))
        .finish()
}

#[get("/console/conversations")]
pub(super) async fn conversations_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/conversations.html"))
}

#[get("/console/families")]
pub(super) async fn families_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/families.html"))
}

#[get("/console/notes")]
pub(super) async fn notes_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/notes.html"))
}

#[actix_web::get("/invite/{token}")]
pub(super) async fn invitation_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/families.html"))
}

#[get("/assets/console.css")]
pub(super) async fn console_stylesheet() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/css; charset=utf-8")
        .body(include_str!("../../static/console.css"))
}

#[get("/assets/console.js")]
pub(super) async fn console_script() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/javascript; charset=utf-8")
        .body(include_str!("../../static/console.js"))
}

#[get("/favicon.ico")]
pub(super) async fn favicon() -> impl Responder {
    HttpResponse::NoContent().finish()
}
