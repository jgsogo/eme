//! Serves the independent desktop and mobile web console pages and their shared assets.

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use actix_web::{
    HttpRequest, HttpResponse, Responder, get,
    http::header,
    post,
    web::{self, Data},
};
use agent_models::{ConversationMessage, ConversationRequest, ModelError, Registry};
use agent_runtime::ToolDefinition;
use agent_server::auth::TelegramAuth;

fn login_return_to(request: &HttpRequest) -> String {
    let query = web::Query::<HashMap<String, String>>::from_query(request.query_string()).ok();
    let destination = query
        .as_ref()
        .and_then(|query| query.get("return_to"))
        .map(String::as_str)
        .unwrap_or("/");
    if destination.starts_with('/')
        && !destination.starts_with("//")
        && !destination.starts_with("/login")
        && !destination.contains('\\')
        && !destination.chars().any(char::is_control)
    {
        destination.to_owned()
    } else {
        "/".to_owned()
    }
}

#[get("/login")]
pub(super) async fn login_page(request: HttpRequest, auth: Data<TelegramAuth>) -> impl Responder {
    let destination = login_return_to(&request);
    if auth.owner_for_request(&request).is_some() {
        return HttpResponse::Found()
            .append_header((header::LOCATION, destination))
            .finish();
    }
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/login.html"))
}

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

#[get("/console/admin/usage")]
pub(super) async fn usage_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/usage.html"))
}

#[get("/console/admin/providers")]
pub(super) async fn providers_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/providers.html"))
}

#[get("/assets/providers.js")]
pub(super) async fn providers_script() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/javascript; charset=utf-8")
        .body(include_str!("../../static/providers.js"))
}

#[get("/providers")]
pub(super) async fn list_configured_providers(providers: Data<Arc<Registry>>) -> HttpResponse {
    HttpResponse::Ok().json(providers.catalog())
}

#[post("/providers")]
pub(super) async fn test_configured_providers(
    providers: Data<Arc<Registry>>,
    body: Option<web::Json<serde_json::Value>>,
) -> HttpResponse {
    let Some(body) = body else {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "A JSON body with a provider name is required"
        }));
    };
    let Some(provider_name) = body.get("provider").and_then(serde_json::Value::as_str) else {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "The provider field must be a string"
        }));
    };
    if provider_name.trim().is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "The provider field cannot be empty"
        }));
    }

    let Some(provider) = providers
        .catalog()
        .into_iter()
        .find(|item| item.name == provider_name)
    else {
        return HttpResponse::NotFound().json(serde_json::json!({
            "error": format!("Provider '{provider_name}' is not configured")
        }));
    };

    let requested_model = match body.get("model") {
        Some(value) => match value.as_str() {
            Some(model) if !model.trim().is_empty() => Some(model.to_owned()),
            _ => {
                return HttpResponse::BadRequest().json(serde_json::json!({
                    "error": "The model field must be a non-empty string when provided"
                }));
            }
        },
        None => None,
    };

    let models = if let Some(model) = requested_model {
        if !provider
            .models
            .iter()
            .any(|configured| configured.name == model)
        {
            return HttpResponse::NotFound().json(serde_json::json!({
                "error": format!("Model '{provider_name}/{model}' is not configured")
            }));
        }
        vec![model]
    } else {
        provider
            .models
            .into_iter()
            .map(|model| model.name)
            .collect()
    };

    let mut results = Vec::with_capacity(models.len());
    for model in models {
        results.push(test_configured_model(providers.get_ref(), provider_name, &model).await);
    }

    HttpResponse::Ok().json(serde_json::json!({
        "provider": provider_name,
        "results": results
    }))
}

async fn test_configured_model(
    registry: &Registry,
    provider: &str,
    model: &str,
) -> serde_json::Value {
    let started = Instant::now();
    let model_id = format!("{provider}/{model}");
    let client = match registry.resolve_model(&model_id) {
        Ok(client) => client,
        Err(error) => {
            return serde_json::json!({
                "model": model,
                "success": false,
                "latency_ms": started.elapsed().as_millis(),
                "error": safe_model_error(&error)
            });
        }
    };

    // Reasoning models may spend a substantial part of their completion budget
    // on internal reasoning before producing visible text.
    const TEST_MAX_OUTPUT_TOKENS: u32 = 256;
    let request = ConversationRequest {
        system: None,
        messages: vec![ConversationMessage::User {
            content: "Reply with exactly: EME model test OK".to_owned(),
        }],
        tools: Vec::new(),
        temperature: Some(0.0),
        max_tokens: Some(TEST_MAX_OUTPUT_TOKENS),
    };

    match tokio::time::timeout(
        Duration::from_secs(30),
        client.complete_conversation(request),
    )
    .await
    {
        Ok(Ok(response)) => {
            let provider_model = response.model;
            let tool_calls = response
                .tool_calls
                .iter()
                .map(|call| call.name.clone())
                .collect::<Vec<_>>();
            let output_tokens = response.usage.as_ref().map(|usage| usage.output_tokens);
            let reasoning_tokens = response
                .usage
                .as_ref()
                .and_then(|usage| usage.reasoning_tokens);
            let usage = response.usage.map(|usage| {
                serde_json::json!({
                    "input_tokens": usage.input_tokens,
                    "output_tokens": usage.output_tokens,
                    "cached_input_tokens": usage.cached_input_tokens,
                    "reasoning_tokens": usage.reasoning_tokens
                })
            });

            match response
                .content
                .filter(|content| !content.trim().is_empty())
            {
                Some(content) => serde_json::json!({
                    "model": model,
                    "provider_model": provider_model,
                    "success": true,
                    "latency_ms": started.elapsed().as_millis(),
                    "response": content,
                    "usage": usage
                }),
                None => {
                    let error = if tool_calls.is_empty()
                        && output_tokens.is_some_and(|tokens| tokens >= TEST_MAX_OUTPUT_TOKENS)
                    {
                        format!(
                            "The model exhausted the {TEST_MAX_OUTPUT_TOKENS}-token completion budget without returning text{}",
                            reasoning_tokens
                                .map(|tokens| format!(" ({tokens} tokens used for reasoning)"))
                                .unwrap_or_default()
                        )
                    } else if tool_calls.is_empty() {
                        "The provider returned an empty text response without exhausting the completion budget".to_owned()
                    } else {
                        format!(
                            "The provider returned no text content and emitted {} tool call(s): {}",
                            tool_calls.len(),
                            tool_calls.join(", ")
                        )
                    };

                    serde_json::json!({
                        "model": model,
                        "success": false,
                        "latency_ms": started.elapsed().as_millis(),
                        "error": error,
                        "diagnostics": {
                            "provider_model": provider_model,
                            "tool_calls": tool_calls,
                            "usage": usage
                        }
                    })
                }
            }
        }
        Ok(Err(error)) => serde_json::json!({
            "model": model,
            "success": false,
            "latency_ms": started.elapsed().as_millis(),
            "error": safe_model_error(&error)
        }),
        Err(_) => serde_json::json!({
            "model": model,
            "success": false,
            "latency_ms": started.elapsed().as_millis(),
            "error": "Timed out after 30 seconds"
        }),
    }
}

fn safe_model_error(error: &ModelError) -> String {
    match error {
        ModelError::InvalidModelName(_) => "Invalid model identifier".to_owned(),
        ModelError::MissingApiKey(variable) => {
            format!("Missing required environment variable '{variable}'")
        }
        ModelError::UnsupportedProvider(provider) => {
            format!("Unsupported provider '{provider}'")
        }
        ModelError::ProviderDisabled(provider) => {
            format!("Provider '{provider}' is disabled")
        }
        ModelError::ModelDisabled(model) => format!("Model '{model}' is disabled"),
        ModelError::UnsupportedModel(model) => format!("Unsupported model '{model}'"),
        ModelError::Http(_) => {
            "Provider request failed due to a network or transport error".to_owned()
        }
        ModelError::Provider(_) => "The provider rejected the request".to_owned(),
        ModelError::ProviderResponse { status, .. } => {
            format!("The provider returned HTTP {status}")
        }
        ModelError::InvalidResponse(_) => "The provider returned an invalid response".to_owned(),
        ModelError::Cancelled => "The model test was cancelled".to_owned(),
        ModelError::AllCandidatesFailed(_) => "The model test failed".to_owned(),
    }
}

#[get("/console/admin/agents")]
pub(super) async fn agents_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/agents.html"))
}

#[get("/assets/agents.js")]
pub(super) async fn agents_script() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/javascript; charset=utf-8")
        .body(include_str!("../../static/agents.js"))
}

#[get("/console/admin/tools")]
pub(super) async fn tools_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/tools.html"))
}

#[get("/assets/tools.js")]
pub(super) async fn tools_script() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/javascript; charset=utf-8")
        .body(include_str!("../../static/tools.js"))
}

#[get("/tools")]
pub(super) async fn list_registered_tools(tools: Data<Vec<ToolDefinition>>) -> impl Responder {
    HttpResponse::Ok().json(
        tools
            .iter()
            .map(|tool| {
                serde_json::json!({
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters,
                })
            })
            .collect::<Vec<_>>(),
    )
}

#[get("/assets/usage.js")]
pub(super) async fn usage_script() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/javascript; charset=utf-8")
        .body(include_str!("../../static/usage.js"))
}

#[get("/assets/usage.css")]
pub(super) async fn usage_stylesheet() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/css; charset=utf-8")
        .body(include_str!("../../static/usage.css"))
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

#[get("/console/books")]
pub(super) async fn books_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/books.html"))
}

#[get("/console/books/libraries")]
pub(super) async fn books_libraries_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/books_libraries.html"))
}

#[get("/assets/libraries.js")]
pub(super) async fn libraries_script() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/javascript; charset=utf-8")
        .body(include_str!("../../static/libraries.js"))
}

#[get("/assets/books.js")]
pub(super) async fn books_script() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/javascript; charset=utf-8")
        .body(include_str!("../../static/books.js"))
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

#[get("/assets/desktop-monthly-planner.js")]
pub(super) async fn desktop_monthly_planner_script() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/javascript; charset=utf-8")
        .body(include_str!("../../static/desktop-monthly-planner.js"))
}

#[get("/assets/desktop-monthly-school-menus.js")]
pub(super) async fn desktop_monthly_school_menus_script() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/javascript; charset=utf-8")
        .body(include_str!("../../static/desktop-monthly-school-menus.js"))
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

#[get("/mobile/admin/usage")]
pub(super) async fn mobile_admin_usage_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/mobile_admin_usage.html"))
}

#[get("/mobile/admin/providers")]
pub(super) async fn mobile_admin_providers_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/mobile_admin_providers.html"))
}

#[get("/mobile/admin/agents")]
pub(super) async fn mobile_admin_agents_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/mobile_admin_agents.html"))
}

#[get("/mobile/admin/tools")]
pub(super) async fn mobile_admin_tools_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/mobile_admin_tools.html"))
}

#[get("/assets/mobile-admin.css")]
pub(super) async fn mobile_admin_stylesheet() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/css; charset=utf-8")
        .body(include_str!("../../static/mobile-admin.css"))
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

#[get("/mobile/books")]
pub(super) async fn mobile_books_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/mobile_books.html"))
}

#[get("/mobile/books/libraries")]
pub(super) async fn mobile_books_libraries_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/mobile_books_libraries.html"))
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

#[get("/mobile/integrations")]
pub(super) async fn mobile_integrations_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/mobile_integrations.html"))
}

#[get("/mobile/meals")]
pub(super) async fn mobile_meals_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/mobile_meals.html"))
}
#[get("/mobile/meal-planner")]
pub(super) async fn mobile_meal_planner_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/mobile_meal_planner.html"))
}
#[get("/mobile/shopping-list")]
pub(super) async fn mobile_shopping_list_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/mobile_shopping_list.html"))
}
#[get("/mobile/school-menus")]
pub(super) async fn mobile_school_menus_page() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/mobile_school_menus.html"))
}
#[get("/assets/mobile-base.css")]
pub(super) async fn mobile_base_stylesheet() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/css; charset=utf-8")
        .body(include_str!("../../static/mobile-base.css"))
}

#[get("/assets/login.js")]
pub(super) async fn login_script() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/javascript; charset=utf-8")
        .body(include_str!("../../static/login.js"))
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
    use super::{
        is_mobile_user_agent, list_configured_providers, login_return_to, test_configured_providers,
    };

    #[test]
    fn login_return_to_accepts_only_local_destinations() {
        use actix_web::test;

        let request =
            test::TestRequest::with_uri("/login?return_to=%2Fconsole%2Ftasks%3Fstatus%3Dfailed")
                .to_http_request();
        assert_eq!(login_return_to(&request), "/console/tasks?status=failed");

        for uri in [
            "/login?return_to=%2F%2Fevil.example",
            "/login?return_to=javascript%3Aalert(1)",
            "/login?return_to=%2Flogin",
            "/login?return_to=%2F%5C%5Cevil.example",
        ] {
            let request = test::TestRequest::with_uri(uri).to_http_request();
            assert_eq!(login_return_to(&request), "/", "{uri}");
        }
    }

    #[actix_web::test]
    async fn model_test_reports_missing_credentials_for_the_selected_model() {
        use actix_web::{App, http::StatusCode, test, web};
        use agent_models::Registry;
        use serde_json::{Value, json};
        use std::{
            fs,
            sync::Arc,
            time::{SystemTime, UNIX_EPOCH},
        };

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("eme-model-test-{}-{unique}", std::process::id()));
        let providers = directory.join("providers");
        fs::create_dir_all(&providers).unwrap();
        fs::write(
            providers.join("test.toml"),
            r#"
name = "test"
client = "gemini"
enabled = true
api_key_env = "EME_TEST_MISSING_MODEL_KEY"

[[models]]
name = "mock-model"
enabled = true
access_tier = "unknown"
capabilities = ["chat"]
"#,
        )
        .unwrap();

        let registry = Arc::new(Registry::load(&directory).unwrap());
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(registry))
                .service(list_configured_providers)
                .service(test_configured_providers),
        )
        .await;

        let request = test::TestRequest::post()
            .uri("/providers")
            .set_json(json!({"provider": "test", "model": "mock-model"}))
            .to_request();
        let response = test::call_service(&app, request).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = test::read_body_json(response).await;
        assert_eq!(body["results"][0]["model"], "mock-model");
        assert_eq!(body["results"][0]["success"], false);
        assert!(
            body["results"][0]["error"]
                .as_str()
                .unwrap()
                .contains("EME_TEST_MISSING_MODEL_KEY")
        );

        fs::remove_dir_all(directory).unwrap();
    }

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
