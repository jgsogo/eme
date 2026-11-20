//! Serves the independent desktop and mobile web console pages and their shared assets.

use actix_web::{HttpRequest, HttpResponse, Responder, get, http::header};

fn is_mobile_user_agent(user_agent: &str) -> bool {
    let user_agent = user_agent.to_ascii_lowercase();
    ["android", "iphone", "ipad", "ipod", "mobile"]
        .iter()
        .any(|marker| user_agent.contains(marker))
}

fn is_mobile_request(request: &HttpRequest) -> bool {
    request
        .headers()
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(is_mobile_user_agent)
}

#[get("/")]
pub(super) async fn console_root(request: HttpRequest) -> impl Responder {
    let destination = if is_mobile_request(&request) {
        "/mobile/conversations"
    } else {
        "/console/conversations"
    };
    HttpResponse::Found()
        .append_header((header::LOCATION, destination))
        .finish()
}

#[get("/mobile")]
pub(super) async fn mobile_root() -> impl Responder {
    HttpResponse::Found()
        .append_header((header::LOCATION, "/mobile/conversations"))
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

#[get("/console/integrations")]
pub(super) async fn integrations_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/integrations.html"))
}

#[get("/console/profile")]
pub(super) async fn profile_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/profile.html"))
}

#[get("/console/school-menus")]
pub(super) async fn school_menus_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/school_menus.html"))
}
#[get("/assets/meals-family.js")]
pub(super) async fn meals_family_script() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/javascript; charset=utf-8")
        .body(include_str!("../../static/meals-family.js"))
}
#[get("/assets/school-menus.js")]
pub(super) async fn school_menus_script() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/javascript; charset=utf-8")
        .body(include_str!("../../static/school-menus.js"))
}
#[get("/console/meals")]
pub(super) async fn meals_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/recipes.html"))
}

#[get("/console/recipes")]
pub(super) async fn recipes_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/recipes.html"))
}

#[get("/assets/recipes.js")]
pub(super) async fn recipes_script() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/javascript; charset=utf-8")
        .body(include_str!("../../static/recipes.js"))
}

#[get("/console/meal-planner")]
pub(super) async fn weekly_planner_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/weekly_planner.html"))
}

#[get("/assets/weekly-planner.js")]
pub(super) async fn weekly_planner_script() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/javascript; charset=utf-8")
        .body(include_str!("../../static/weekly-planner.js"))
}

#[get("/console/tasks")]
pub(super) async fn tasks_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/tasks.html"))
}

#[actix_web::get("/invite/{token}")]
pub(super) async fn invitation_page(request: HttpRequest) -> impl Responder {
    let page = if is_mobile_request(&request) {
        include_str!("../../static/mobile_families.html")
    } else {
        include_str!("../../static/families.html")
    };
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(page)
}

#[get("/mobile/conversations")]
pub(super) async fn mobile_conversations_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/mobile_conversations.html"))
}

#[get("/mobile/families")]
pub(super) async fn mobile_families_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/mobile_families.html"))
}

#[get("/mobile/notes")]
pub(super) async fn mobile_notes_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/mobile_notes.html"))
}

#[get("/mobile/profile")]
pub(super) async fn mobile_profile_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/mobile_profile.html"))
}

#[get("/mobile/tasks")]
pub(super) async fn mobile_tasks_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/mobile_tasks.html"))
}

#[get("/assets/desktop-header.js")]
pub(super) async fn desktop_header_script() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/javascript; charset=utf-8")
        .body(include_str!("../../static/desktop-header.js"))
}

#[get("/assets/console.css")]
pub(super) async fn console_stylesheet() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/css; charset=utf-8")
        .body(include_str!("../../static/console.css"))
}

#[get("/assets/console-auth.js")]
pub(super) async fn console_auth_script() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/javascript; charset=utf-8")
        .body(include_str!("../../static/console-auth.js"))
}

#[get("/assets/console.js")]
pub(super) async fn console_script() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/javascript; charset=utf-8")
        .body(include_str!("../../static/console.js"))
}

#[get("/assets/mobile.css")]
pub(super) async fn mobile_stylesheet() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/css; charset=utf-8")
        .body(include_str!("../../static/mobile.css"))
}

#[get("/assets/mobile.js")]
pub(super) async fn mobile_script() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/javascript; charset=utf-8")
        .body(include_str!("../../static/mobile.js"))
}

#[get("/favicon.ico")]
pub(super) async fn favicon() -> impl Responder {
    HttpResponse::NoContent().finish()
}

#[cfg(test)]
mod tests {
    use super::is_mobile_user_agent;

    #[test]
    fn detects_common_mobile_user_agents() {
        assert!(is_mobile_user_agent(
            "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0)"
        ));
        assert!(is_mobile_user_agent(
            "Mozilla/5.0 (Linux; Android 15; Pixel 9)"
        ));
        assert!(!is_mobile_user_agent("Mozilla/5.0 (X11; Linux x86_64)"));
    }
}

#[get("/console/shopping-list")]
pub(super) async fn shopping_list_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/shopping_list.html"))
}
#[get("/assets/shopping-list.js")]
pub(super) async fn shopping_list_script() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/javascript; charset=utf-8")
        .body(include_str!("../../static/shopping-list.js"))
}
