//! HTTP middleware enforcing browser authentication and run ownership.

use actix_web::{
    Error, HttpResponse,
    body::{EitherBody, MessageBody},
    dev::{ServiceRequest, ServiceResponse},
    middleware::Next,
    web,
};
use agent_server::auth::TelegramAuth;

pub(super) async fn require_telegram_session<B>(
    request: ServiceRequest,
    next: Next<B>,
) -> Result<ServiceResponse<EitherBody<B>>, Error>
where
    B: MessageBody + 'static,
{
    let path = request.path();
    if path == "/"
        || path == "/console/conversations"
        || path == "/console/families"
        || path == "/console/notes"
        || path.starts_with("/assets/")
        || path == "/favicon.ico"
        || path.starts_with("/auth/")
        || path.starts_with("/invite/")
    {
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
