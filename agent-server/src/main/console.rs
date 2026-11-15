use actix_web::{HttpResponse, Responder, get};

#[get("/")]
pub(super) async fn console_ui() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/index.html"))
}
