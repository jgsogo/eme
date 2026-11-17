//! Serves the independent web console pages and their shared assets.

use actix_web::{HttpRequest, HttpResponse, Responder, get, http::header};

fn is_mobile_user_agent(user_agent: &str) -> bool {
    let user_agent = user_agent.to_ascii_lowercase();
    ["android", "iphone", "ipad", "ipod", "mobile"].iter().any(|marker| user_agent.contains(marker))
}

#[get("/")]
pub(super) async fn console_root(request: HttpRequest) -> impl Responder {
    let user_agent = request.headers().get(header::USER_AGENT).and_then(|value| value.to_str().ok()).unwrap_or_default();
    let destination = if is_mobile_user_agent(user_agent) { "/mobile/conversations" } else { "/console/conversations" };
    HttpResponse::Found().append_header((header::LOCATION, destination)).finish()
}

#[get("/mobile")]
pub(super) async fn mobile_root() -> impl Responder {
    HttpResponse::Found().append_header((header::LOCATION, "/mobile/conversations")).finish()
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

#[get("/console/profile")]
pub(super) async fn profile_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/profile.html"))
}

#[get("/console/tasks")]
pub(super) async fn tasks_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/tasks.html"))
}

#[actix_web::get("/invite/{token}")]
pub(super) async fn invitation_page(request: HttpRequest) -> impl Responder {
    let user_agent = request.headers().get(header::USER_AGENT).and_then(|value| value.to_str().ok()).unwrap_or_default();
    let page = if is_mobile_user_agent(user_agent) { include_str!("../../static/mobile_families.html") } else { include_str!("../../static/families.html") };
    HttpResponse::Ok().content_type("text/html; charset=utf-8").body(page)
}

#[get("/mobile/conversations")]
pub(super) async fn mobile_conversations_page() -> impl Responder {
    HttpResponse::Ok().content_type("text/html; charset=utf-8").body(include_str!("../../static/mobile_conversations.html"))
}

#[get("/mobile/families")]
pub(super) async fn mobile_families_page() -> impl Responder {
    HttpResponse::Ok().content_type("text/html; charset=utf-8").body(include_str!("../../static/mobile_families.html"))
}

#[get("/mobile/notes")]
pub(super) async fn mobile_notes_page() -> impl Responder {
    HttpResponse::Ok().content_type("text/html; charset=utf-8").body(include_str!("../../static/mobile_notes.html"))
}

#[get("/mobile/profile")]
pub(super) async fn mobile_profile_page() -> impl Responder {
    HttpResponse::Ok().content_type("text/html; charset=utf-8").body(include_str!("../../static/mobile_profile.html"))
}

#[get("/mobile/tasks")]
pub(super) async fn mobile_tasks_page() -> impl Responder {
    HttpResponse::Ok().content_type("text/html; charset=utf-8").body(include_str!("../../static/mobile_tasks.html"))
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

#[get("/assets/mobile.css")]
pub(super) async fn mobile_stylesheet() -> impl Responder {
    HttpResponse::Ok().content_type("text/css; charset=utf-8").body(include_str!("../../static/mobile.css"))
}

#[get("/assets/mobile.js")]
pub(super) async fn mobile_script() -> impl Responder {
    HttpResponse::Ok().content_type("text/javascript; charset=utf-8").body(include_str!("../../static/mobile.js"))
}

#[get("/favicon.ico")]
pub(super) async fn favicon() -> impl Responder {
    HttpResponse::NoContent().finish()
}
