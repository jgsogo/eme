use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use actix_web::HttpRequest;
use uuid::Uuid;
use agent_storage::UserIdentityStore;

pub(super) const SESSION_COOKIE: &str = "eme_session";
pub(super) const LOGIN_TTL: Duration = Duration::from_secs(5 * 60);
pub(super) const SESSION_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

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

    pub(super) fn create_login(&self) -> (String, Option<String>) {
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
    pub fn complete_login(
        &self,
        token: &str,
        telegram_user_id: i64,
        identities: &dyn UserIdentityStore,
    ) -> bool {
        if identities
            .resolve_or_create("telegram", &telegram_user_id.to_string(), None)
            .is_err()
        {
            return false;
        }
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
        Self::lock(&self.run_owners)
            .get(run_id)
            .is_some_and(|run_owner| run_owner == owner)
    }

    pub fn runs_for_owner(&self, owner: &str) -> Vec<String> {
        Self::lock(&self.run_owners)
            .iter()
            .filter(|(_, run_owner)| run_owner.as_str() == owner)
            .map(|(run_id, _)| run_id.clone())
            .collect()
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

    pub(super) fn cookie(&self, token: &str, max_age: u64) -> String {
        let secure = if self.cookie_secure { "; Secure" } else { "" };
        format!(
            "{SESSION_COOKIE}={token}; Path=/; Max-Age={max_age}; HttpOnly; SameSite=Lax{secure}"
        )
    }

    pub(super) fn clear_cookie(&self) -> String {
        let secure = if self.cookie_secure { "; Secure" } else { "" };
        format!("{SESSION_COOKIE}=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax{secure}")
    }

    pub(super) fn poll_login(&self, token: &str) -> Option<String> {
        let mut pending = Self::lock(&self.pending);
        pending.retain(|_, login| login.expires_at > Instant::now());
        let login = pending.get_mut(token)?;
        let session = login.session.take();
        if session.is_some() {
            pending.remove(token);
        }
        session
    }

    pub(super) fn logout(&self, request: &HttpRequest) {
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
