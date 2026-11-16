//! HTTP endpoints for starting, polling, inspecting, and ending sessions.

use actix_web::{
    HttpRequest, HttpResponse, Responder, Scope, get, post,
    web::{self, Data},
};
use serde::{Deserialize, Serialize};

use super::telegram_auth::{LOGIN_TTL, SESSION_TTL, TelegramAuth};

#[derive(Deserialize)]
struct LoginStatusRequest {
    token: String,
}

#[derive(Serialize)]
struct StartLoginResponse {
    token: String,
    bot_url: Option<String>,
    expires_in_seconds: u64,
}

#[derive(Serialize)]
struct SessionResponse {
    owner: String,
}

#[post("/telegram/start")]
async fn start_login(auth: Data<TelegramAuth>) -> impl Responder {
    let (token, bot_url) = auth.create_login();
    HttpResponse::Created().json(StartLoginResponse {
        token,
        bot_url,
        expires_in_seconds: LOGIN_TTL.as_secs(),
    })
}

#[post("/telegram/status")]
async fn login_status(
    auth: Data<TelegramAuth>,
    request: web::Json<LoginStatusRequest>,
) -> impl Responder {
    match auth.poll_login(&request.token) {
        Some(session) => HttpResponse::Ok()
            .insert_header((
                actix_web::http::header::SET_COOKIE,
                auth.cookie(&session, SESSION_TTL.as_secs()),
            ))
            .json(serde_json::json!({ "authenticated": true })),
        None => HttpResponse::Accepted().json(serde_json::json!({ "authenticated": false })),
    }
}

#[get("/session")]
async fn current_session(request: HttpRequest, auth: Data<TelegramAuth>) -> impl Responder {
    match auth.owner_for_request(&request) {
        Some(owner) => HttpResponse::Ok().json(SessionResponse { owner }),
        None => HttpResponse::Unauthorized().finish(),
    }
}

#[post("/logout")]
async fn logout(request: HttpRequest, auth: Data<TelegramAuth>) -> impl Responder {
    auth.logout(&request);
    HttpResponse::NoContent()
        .insert_header((actix_web::http::header::SET_COOKIE, auth.clear_cookie()))
        .finish()
}

pub fn auth_api() -> Scope {
    web::scope("/auth")
        .service(start_login)
        .service(login_status)
        .service(current_session)
        .service(logout)
}
