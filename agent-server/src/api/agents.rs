//! Agent discovery route and response handling.

use actix_web::{
    HttpResponse, Responder, Scope, get,
    web::{self, Data},
};

use super::types::AgentCatalog;

#[get("")]
async fn list_agents(catalog: Data<AgentCatalog>) -> impl Responder {
    HttpResponse::Ok().json(catalog.list())
}

/// Builds the /agents route scope. Register AgentCatalog as application data
/// before mounting this scope.
pub fn agents_api() -> Scope {
    web::scope("/agents").service(list_agents)
}
