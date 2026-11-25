//! HTTP middleware enforcing browser-session authentication.

use actix_web::{
    Error, HttpResponse,
    body::{EitherBody, MessageBody},
    dev::{ServiceRequest, ServiceResponse},
    http::{Method, header},
    middleware::Next,
    web,
};
use agent_server::auth::TelegramAuth;

fn is_public_path(path: &str) -> bool {
    path == "/login"
        || path.starts_with("/assets/")
        || path == "/favicon.ico"
        || path.starts_with("/auth/")
}

fn is_console_page(method: &Method, path: &str) -> bool {
    *method == Method::GET
        && (path == "/"
            || path == "/mobile"
            || path.starts_with("/mobile/")
            || path.starts_with("/console/")
            || path.starts_with("/invite/"))
}

fn login_location(return_to: &str) -> String {
    let encoded = return_to
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b'?' | b'&' | b'=' | b':' | b'@' | b'!' | b'$' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' => (byte as char).to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect::<String>();
    format!("/login?return_to={encoded}")
}

pub(super) async fn require_telegram_session<B>(
    request: ServiceRequest,
    next: Next<B>,
) -> Result<ServiceResponse<EitherBody<B>>, Error>
where
    B: MessageBody + 'static,
{
    let path = request.path();
    if is_public_path(path) {
        return Ok(next.call(request).await?.map_into_left_body());
    }

    let auth = request.app_data::<web::Data<TelegramAuth>>();
    let authenticated =
        auth.is_some_and(|auth| auth.owner_for_request(request.request()).is_some());
    if authenticated {
        return Ok(next.call(request).await?.map_into_left_body());
    }

    if is_console_page(request.method(), path) {
        let destination = login_location(request.uri().to_string().as_str());
        return Ok(request
            .into_response(
                HttpResponse::Found()
                    .append_header((header::LOCATION, destination))
                    .finish(),
            )
            .map_into_right_body());
    }

    Ok(request
        .into_response(HttpResponse::Unauthorized().finish())
        .map_into_right_body())
}

#[cfg(test)]
mod tests {
    use actix_web::http::Method;

    use super::{is_console_page, is_public_path, login_location};

    #[test]
    fn login_page_and_auth_assets_are_public() {
        assert!(is_public_path("/login"));
        assert!(is_public_path("/assets/login.js"));
        assert!(is_public_path("/auth/session"));
        assert!(!is_public_path("/console/conversations"));
    }

    #[test]
    fn unauthenticated_console_pages_redirect_but_apis_do_not() {
        for path in [
            "/",
            "/mobile",
            "/mobile/conversations",
            "/console/conversations",
            "/console/admin/usage",
            "/invite/0123456789",
        ] {
            assert!(is_console_page(&Method::GET, path), "{path} should redirect to login");
        }
        assert!(!is_console_page(&Method::POST, "/console/conversations"));
        for path in ["/conversations", "/meal-planner/recipes", "/school-menus", "/tasks", "/runs/123", "/providers"] {
            assert!(!is_console_page(&Method::GET, path), "{path} is an API, not a page");
        }
    }

    #[test]
    fn login_redirect_preserves_the_requested_path_and_query() {
        assert_eq!(
            login_location("/console/tasks?status=failed&limit=20"),
            "/login?return_to=/console/tasks?status=failed&limit=20"
        );
        assert_eq!(
            login_location("/invite/a token"),
            "/login?return_to=/invite/a%20token"
        );
    }
}
