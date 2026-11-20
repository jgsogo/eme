//! HTTP middleware enforcing browser authentication and run ownership.

use actix_web::{
    Error, HttpResponse,
    body::{EitherBody, MessageBody},
    dev::{ServiceRequest, ServiceResponse},
    middleware::Next,
    web,
};
use agent_server::auth::TelegramAuth;

fn is_public_path(path: &str) -> bool {
    path == "/"
        || path == "/mobile"
        || path.starts_with("/mobile/")
        // Desktop HTML routes must be reachable before the browser can run
        // console-auth.js. Keep the data APIs protected below.
        || path.starts_with("/console/")
        || path.starts_with("/assets/")
        || path == "/favicon.ico"
        || path.starts_with("/auth/")
        || path.starts_with("/invite/")
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
    let owner = auth.and_then(|auth| auth.owner_for_request(request.request()));
    let Some(owner) = owner else {
        return Ok(request
            .into_response(HttpResponse::Unauthorized().finish())
            .map_into_right_body());
    };

    if let Some(run_id) = path
        .strip_prefix("/runs/")
        .and_then(|path| path.split('/').next())
    {
        let owns_run = auth.is_some_and(|auth| auth.owns_run(run_id, &owner));
        if !owns_run {
            return Ok(request
                .into_response(HttpResponse::NotFound().finish())
                .map_into_right_body());
        }
    }

    Ok(next.call(request).await?.map_into_left_body())
}

#[cfg(test)]
mod tests {
    use super::is_public_path;

    #[test]
    fn all_desktop_console_pages_are_public_shells() {
        for path in [
            "/console/conversations",
            "/console/meals",
            "/console/recipes",
            "/console/families",
            "/console/notes",
            "/console/profile",
            "/console/integrations",
            "/console/tasks",
            "/console/school-menus",
            "/console/meal-planner",
            "/console/shopping-list",
        ] {
            assert!(is_public_path(path), "{path} should serve the login shell");
        }
    }

    #[test]
    fn application_apis_remain_protected() {
        for path in [
            "/conversations",
            "/meal-planner/recipes",
            "/school-menus",
            "/tasks",
            "/runs/123",
        ] {
            assert!(!is_public_path(path), "{path} must require authentication");
        }
    }
}
