//! HTTP middleware enforcing browser authentication and run ownership.

use actix_web::{
    Error, HttpResponse,
    body::{EitherBody, MessageBody},
    dev::{ServiceRequest, ServiceResponse},
    middleware::Next,
    web,
};
use agent_server::auth::TelegramAuth;
use agent_service::RunId;
use std::str::FromStr;

/// Returns the run ID in a /runs/{run_id}[/*] path.
///
/// Collection endpoints such as /runs/usage and /runs/pricing are not run
/// IDs, so only UUID-shaped path segments are subject to run ownership checks.
fn run_id_from_path(path: &str) -> Option<&str> {
    let segment = path.strip_prefix("/runs/")?.split('/').next()?;
    RunId::from_str(segment).ok()?;
    Some(segment)
}

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

    if let Some(run_id) = run_id_from_path(path) {
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
    use super::{is_public_path, run_id_from_path};

    #[test]
    fn usage_and_pricing_collection_routes_are_not_run_ids() {
        assert_eq!(run_id_from_path("/runs/usage"), None);
        assert_eq!(run_id_from_path("/runs/pricing"), None);
    }

    #[test]
    fn uuid_run_routes_are_subject_to_ownership_checks() {
        let run_id = "550e8400-e29b-41d4-a716-446655440000";
        assert_eq!(run_id_from_path(&format!("/runs/{run_id}")), Some(run_id));
        assert_eq!(
            run_id_from_path(&format!("/runs/{run_id}/usage")),
            Some(run_id)
        );
        assert_eq!(run_id_from_path("/runs/not-a-uuid/usage"), None);
    }

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
