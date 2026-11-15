//! Telegram-based web authentication and browser sessions.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use actix_web::{
    HttpRequest, HttpResponse, Responder, Scope, get, post,
    web::{self, Data},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const SESSION_COOKIE: &str = "eme_session";
const LOGIN_TTL: Duration = Duration::from_secs(5 * 60);
const SESSION_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

struct PendingLogin {
    expires_at: Instant,
    session: Option<String>,
}

struct Session {
    owner: String,
    expires_at: Instant,
}

/// Coordinates one-time Telegram login challenges and opaque browser sessions.
///
/// Sessions are intentionally process-local in this first implementation.
/// A server restart invalidates browser sessions but does not affect stored
/// conversations.
pub struct TelegramAuth {
    bot_username: Option<String>,
    cookie_secure: bool,
    pending: Mutex<HashMap<String, PendingLogin>>,
    sessions: Mutex<HashMap<String, Session>>,
    run_owners: Mutex<HashMap<String, String>>,
}

impl TelegramAuth {
    pub fn new(bot_username: Option<String>, cookie_secure: bool) -> Self {
        Self {
            bot_username: bot_username
                .map(|name| name.trim().trim_start_matches('@').to_owned())
                .filter(|name| !name.is_empty()),
            cookie_secure,
            pending: Mutex::new(HashMap::new()),
            sessions: Mutex::new(HashMap::new()),
            run_owners: Mutex::new(HashMap::new()),
        }
    }

    fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
        mutex
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn create_login(&self) -> (String, Option<String>) {
        let token = Uuid::new_v4().simple().to_string();
        let mut pending = Self::lock(&self.pending);
        pending.retain(|_, login| login.expires_at > Instant::now());
        pending.insert(
            token.clone(),
            PendingLogin {
                expires_at: Instant::now() + LOGIN_TTL,
                session: None,
            },
        );
        let url = self
            .bot_username
            .as_ref()
            .map(|username| format!("https://t.me/{username}?start=auth_{token}"));
        (token, url)
    }

    /// Completes a pending login using the Telegram numeric user ID received
    /// from the Bot API update, never a value supplied by the browser.
    pub fn complete_login(&self, token: &str, telegram_user_id: i64) -> bool {
        let mut pending = Self::lock(&self.pending);
        let Some(login) = pending.get_mut(token) else {
            return false;
        };
        if login.expires_at <= Instant::now() || login.session.is_some() {
            pending.remove(token);
            return false;
        }

        let session_token = Uuid::new_v4().simple().to_string();
        let owner = format!("telegram:{telegram_user_id}");
        Self::lock(&self.sessions).insert(
            session_token.clone(),
            Session {
                owner,
                expires_at: Instant::now() + SESSION_TTL,
            },
        );
        login.session = Some(session_token);
        true
    }

    pub fn register_run(&self, run_id: impl Into<String>, owner: &str) {
        Self::lock(&self.run_owners).insert(run_id.into(), owner.to_owned());
    }

    pub fn owns_run(&self, run_id: &str, owner: &str) -> bool {
        Self::lock(&self.run_owners).get(run_id).is_some_and(|run_owner| run_owner == owner)
    }

    pub fn runs_for_owner(&self, owner: &str) -> Vec<String> {
        Self::lock(&self.run_owners).iter().filter(|(_, run_owner)| run_owner.as_str() == owner).map(|(run_id, _)| run_id.clone()).collect()
    }

    pub fn owner_for_request(&self, request: &HttpRequest) -> Option<String> {
        let cookie_header = request.headers().get(actix_web::http::header::COOKIE)?;
        let cookie_header = cookie_header.to_str().ok()?;
        let token = cookie_header.split(';').find_map(|part| {
            let (name, value) = part.trim().split_once('=')?;
            (name == SESSION_COOKIE && !value.is_empty()).then_some(value)
        })?;

        let mut sessions = Self::lock(&self.sessions);
        sessions.retain(|_, session| session.expires_at > Instant::now());
        sessions.get(token).map(|session| session.owner.clone())
    }

    fn cookie(&self, token: &str, max_age: u64) -> String {
        let secure = if self.cookie_secure { "; Secure" } else { "" };
        format!(
            "{SESSION_COOKIE}={token}; Path=/; Max-Age={max_age}; HttpOnly; SameSite=Lax{secure}"
        )
    }

    fn clear_cookie(&self) -> String {
        let secure = if self.cookie_secure { "; Secure" } else { "" };
        format!("{SESSION_COOKIE}=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax{secure}")
    }

    fn poll_login(&self, token: &str) -> Option<String> {
        let mut pending = Self::lock(&self.pending);
        pending.retain(|_, login| login.expires_at > Instant::now());
        let login = pending.get_mut(token)?;
        let session = login.session.take();
        if session.is_some() {
            pending.remove(token);
        }
        session
    }

    fn logout(&self, request: &HttpRequest) {
        let cookie_header = request
            .headers()
            .get(actix_web::http::header::COOKIE)
            .and_then(|value| value.to_str().ok());
        if let Some(token) = cookie_header.and_then(|header| {
            header.split(';').find_map(|part| {
                let (name, value) = part.trim().split_once('=')?;
                (name == SESSION_COOKIE).then_some(value)
            })
        }) {
            Self::lock(&self.sessions).remove(token);
        }
    }
}

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
