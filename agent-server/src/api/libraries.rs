//! Authenticated RABEL library discovery and per-user preferred-library settings.
use std::sync::Arc;

use actix_web::{
    HttpRequest, HttpResponse, Responder, Scope, get, put,
    web::{self, Data, Json},
};
use agent_storage::FileUserIdentityStore;
use books_tool::BookStore;
use rabel_tool::RabelClient;
use serde::Deserialize;

use super::books::{actor, book_error};

#[derive(Deserialize)]
struct LibrarySelection {
    library_ids: Vec<String>,
}

#[get("")]
async fn list(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    store: Data<BookStore>,
) -> impl Responder {
    let user = match actor(&request, &auth, &identities) {
        Ok(user) => user,
        Err(response) => return response,
    };
    let client = match RabelClient::new() {
        Ok(client) => client,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };
    let available = match client.libraries().await {
        Ok(available) => available,
        Err(error) => return HttpResponse::BadGateway().body(error.to_string()),
    };
    let selected = match store.preferred_libraries(&user).await {
        Ok(selected) => selected,
        Err(error) => return book_error(error),
    };
    HttpResponse::Ok().json(serde_json::json!({ "available": available, "selected": selected }))
}

#[put("")]
async fn save(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    store: Data<BookStore>,
    body: Json<LibrarySelection>,
) -> impl Responder {
    let user = match actor(&request, &auth, &identities) {
        Ok(user) => user,
        Err(response) => return response,
    };
    let client = match RabelClient::new() {
        Ok(client) => client,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };
    let available = match client.libraries().await {
        Ok(available) => available,
        Err(error) => return HttpResponse::BadGateway().body(error.to_string()),
    };
    // Retain an existing preference if RABEL temporarily omits it from its
    // selector response. Unknown IDs that were never saved remain rejected.
    let previous = match store.preferred_libraries(&user).await {
        Ok(previous) => previous,
        Err(error) => return book_error(error),
    };
    let mut selected = Vec::new();
    for id in &body.library_ids {
        if let Some(library) = available.iter().find(|library| &library.id == id) {
            selected.push((library.id.clone(), library.name.clone()));
        } else if let Some((saved_id, saved_name)) =
            previous.iter().find(|(saved_id, _)| saved_id == id)
        {
            selected.push((saved_id.clone(), saved_name.clone()));
        } else {
            return HttpResponse::BadRequest().body(format!("unknown RABEL library id: {id}"));
        }
    }
    match store.set_preferred_libraries(&user, &selected).await {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({ "selected": selected })),
        Err(error) => book_error(error),
    }
}

pub fn libraries_api() -> Scope {
    web::scope("/libraries").service(list).service(save)
}
