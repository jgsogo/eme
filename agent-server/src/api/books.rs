//! Authenticated CRUD API for the current user's personal book collection.
use std::sync::Arc;
use actix_web::{HttpRequest,HttpResponse,Responder,Scope,delete,get,post,put,web::{self,Data,Json,Path}};
use agent_storage::{FileUserIdentityStore,UserIdentityStore};
use books_tool::{BookInput,BookStore,BookStoreError,BookUpdate};
use rabel_tool::{RabelClient, RabelError};
use serde::Deserialize;
pub(super) fn actor(r:&HttpRequest,a:&crate::auth::TelegramAuth,i:&FileUserIdentityStore)->Result<String,HttpResponse>{
 let owner=a.owner_for_request(r).ok_or_else(||HttpResponse::Unauthorized().finish())?;
 let subject=owner.strip_prefix("telegram:").filter(|v|!v.is_empty()).ok_or_else(||HttpResponse::Unauthorized().finish())?;
 let identity=i.find_identity("telegram",subject).map_err(|_|HttpResponse::InternalServerError().finish())?.ok_or_else(||HttpResponse::Unauthorized().finish())?;
 Ok(identity.user_id.to_string())
}
pub(super) fn book_error(e:BookStoreError)->HttpResponse{match e{BookStoreError::NotFound=>HttpResponse::NotFound().finish(),BookStoreError::Invalid(m)=>HttpResponse::BadRequest().body(m),_=>HttpResponse::InternalServerError().finish()}}
#[derive(Deserialize)]
struct CatalogueSearchQuery { q: String }

#[get("/catalogue/search")]
async fn search_catalogue(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    query: web::Query<CatalogueSearchQuery>,
) -> impl Responder {
    if let Err(response) = actor(&request, &auth, &identities) { return response; }
    if query.q.trim().is_empty() { return HttpResponse::BadRequest().body("q must not be empty"); }
    let client = match RabelClient::new() {
        Ok(client) => client,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };
    match client.search(&query.q, 20).await {
        Ok(results) => HttpResponse::Ok().json(results),
        Err(error) => {
            // Keep the response useful without exposing the searched term or
            // session-scoped OPAC URL (both can appear in reqwest diagnostics).
            let message = match error {
                RabelError::Http(status) => format!("RABEL returned HTTP {status}"),
                RabelError::Request(_) => "Could not connect to RABEL".to_owned(),
                RabelError::Parse(diagnostic) => format!("RABEL returned an unexpected catalogue response: {diagnostic}"),
                RabelError::InvalidRecordReference => "RABEL returned an invalid record reference".to_owned(),
            };
            tracing::warn!(error = %message, "RABEL catalogue search failed");
            HttpResponse::BadGateway().body(message)
        },
    }
}

#[get("/catalogue/item")]
async fn catalogue_item(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> impl Responder {
    if let Err(response) = actor(&request, &auth, &identities) { return response; }
    let Some(record_ref) = query.get("record_ref").filter(|value| !value.trim().is_empty()) else {
        return HttpResponse::BadRequest().body("record_ref must not be empty");
    };
    let client = match RabelClient::new() {
        Ok(client) => client,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };
    match client.get_item(record_ref).await {
        Ok(item) => HttpResponse::Ok().json(item),
        Err(RabelError::Http(status)) => HttpResponse::BadGateway().body(format!("RABEL returned HTTP {status}")),
        Err(RabelError::Request(_)) => HttpResponse::BadGateway().body("Could not connect to RABEL"),
        Err(RabelError::Parse(_)) => HttpResponse::BadGateway().body("RABEL returned an unexpected record response"),
        Err(RabelError::InvalidRecordReference) => HttpResponse::BadRequest().body("Invalid RABEL record reference"),
    }
}

#[post("/{id}/refresh-catalogue")]
async fn refresh_catalogue(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    store: Data<BookStore>,
    id: Path<String>,
) -> impl Responder {
    let user = match actor(&request, &auth, &identities) {
        Ok(user) => user,
        Err(response) => return response,
    };
    let book = match store.get(&user, &id).await {
        Ok(book) => book,
        Err(error) => return book_error(error),
    };
    let client = match RabelClient::new() {
        Ok(client) => client,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };
    let query = book.isbn.as_deref().unwrap_or(&book.title);
    let results = match client.search(query, 20).await {
        Ok(results) => results,
        Err(error) => {
            tracing::warn!(error = %error, book_id = %book.id, "RABEL catalogue refresh search failed");
            return HttpResponse::BadGateway().body("Could not search RABEL for this book");
        }
    };

    let wanted_isbn = book.isbn.as_deref().map(normalize_isbn);
    let wanted_title = normalize_title(&book.title);
    let mut matched_item = None;
    for result in results {
        if wanted_isbn.as_ref().is_some_and(|wanted| {
            result.isbn.as_deref().is_some_and(|candidate| normalize_isbn(candidate) != *wanted)
        }) {
            continue;
        }
        let item = match client.get_item(&result.record_ref).await {
            Ok(item) => item,
            Err(error) => {
                tracing::debug!(error = %error, "Skipping unreadable RABEL record during refresh");
                continue;
            }
        };
        let matches = if let Some(wanted) = &wanted_isbn {
            item.isbn.as_deref().is_some_and(|candidate| normalize_isbn(candidate) == *wanted)
                || result.isbn.as_deref().is_some_and(|candidate| normalize_isbn(candidate) == *wanted)
        } else {
            item.title.as_deref().is_some_and(|title| normalize_title(title) == wanted_title)
        };
        if matches {
            matched_item = Some(item);
            break;
        }
    }

    let Some(item) = matched_item else {
        return HttpResponse::NotFound().body("No exact matching RABEL record was found; collection data was left unchanged");
    };
    let update = BookUpdate {
        title: item.title.clone(),
        authors: Some(item.authors.clone()),
        isbn: Some(item.isbn.clone()),
        publisher: Some(item.publisher.clone()),
        publication_year: Some(item.publication_year),
        responsibility: Some(item.responsibility.clone()),
        publication_statement: Some(item.publication_statement.clone()),
        physical_description: Some(item.physical_description.clone()),
        series: Some(item.series.clone()),
        notes: Some(item.notes.clone()),
        subjects: Some(item.subjects.clone()),
        ..Default::default()
    };
    match store.update(&user, &book.id, update).await {
        Ok(updated) => HttpResponse::Ok().json(serde_json::json!({
            "book": updated,
            "catalogue_item": item
        })),
        Err(error) => book_error(error),
    }
}

#[get("/{id}/holdings")]
async fn holdings(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    store: Data<BookStore>,
    id: Path<String>,
) -> impl Responder {
    let user = match actor(&request, &auth, &identities) {
        Ok(user) => user,
        Err(response) => return response,
    };
    let book = match store.get(&user, &id).await {
        Ok(book) => book,
        Err(error) => return book_error(error),
    };
    let selected_libraries = match store.preferred_libraries(&user).await {
        Ok(selected_libraries) => selected_libraries,
        Err(store_error) => return book_error(store_error),
    };
    let mut library_results = Vec::with_capacity(selected_libraries.len());
    for (library_id, library_name) in selected_libraries {
        let client = match RabelClient::for_subcatalog(&library_id) {
            Ok(client) => client,
            Err(error) => {
                library_results.push(serde_json::json!({
                    "library_id": library_id,
                    "library_name": library_name,
                    "status": "error",
                    "error": error.to_string(),
                    "copies": []
                }));
                continue;
            }
        };
        let query = book.isbn.as_deref().unwrap_or(&book.title);
        let results = match client.search(query, 20).await {
            Ok(results) => results,
            Err(error) => {
                library_results.push(serde_json::json!({
                    "library_id": library_id,
                    "library_name": library_name,
                    "status": "error",
                    "error": error.to_string(),
                    "copies": []
                }));
                continue;
            }
        };
        let wanted_isbn = book.isbn.as_deref().map(normalize_isbn);
        let wanted_title = normalize_title(&book.title);
        let record = results.iter().find(|result| {
            wanted_isbn.as_ref().is_some_and(|isbn| {
                result.isbn.as_deref().is_some_and(|candidate| normalize_isbn(candidate).as_str() == isbn.as_str())
            })
        }).or_else(|| {
            // ISBN is preferred, but some catalogue records omit it. Fall back
            // only to an exact normalized title match, never the first hit.
            results.iter().find(|result| normalize_title(&result.title) == wanted_title)
        }).cloned();
        let Some(record) = record else {
            library_results.push(serde_json::json!({
                "library_id": library_id,
                "library_name": library_name,
                "status": "not_found",
                "copies": []
            }));
            continue;
        };
        match client.get_holdings(&record.record_ref).await {
            Ok(holdings) => library_results.push(serde_json::json!({
                "library_id": library_id,
                "library_name": library_name,
                "status": "found",
                "catalogue_title": record.title,
                "checked_at": holdings.checked_at,
                "copies": holdings.copies
            })),
            Err(error) => library_results.push(serde_json::json!({
                "library_id": library_id,
                "library_name": library_name,
                "status": "error",
                "error": error.to_string(),
                "copies": []
            })),
        }
    }
    HttpResponse::Ok().json(serde_json::json!({
        "book_id": book.id,
        "libraries": library_results
    }))
}

fn normalize_isbn(value: &str) -> String {
    value.chars().filter(|character| character.is_ascii_alphanumeric())
        .map(|character| character.to_ascii_uppercase()).collect()
}

fn normalize_title(value: &str) -> String {
    value.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}

#[get("")] async fn list(r:HttpRequest,a:Data<crate::auth::TelegramAuth>,i:Data<Arc<FileUserIdentityStore>>,s:Data<BookStore>)->impl Responder{let u=match actor(&r,&a,&i){Ok(v)=>v,Err(e)=>return e};match s.list(&u).await{Ok(v)=>HttpResponse::Ok().json(v),Err(e)=>book_error(e)}}
#[post("")] async fn add(r:HttpRequest,a:Data<crate::auth::TelegramAuth>,i:Data<Arc<FileUserIdentityStore>>,s:Data<BookStore>,b:Json<BookInput>)->impl Responder{let u=match actor(&r,&a,&i){Ok(v)=>v,Err(e)=>return e};match s.add(&u,b.into_inner()).await{Ok(v)=>HttpResponse::Created().json(v),Err(e)=>book_error(e)}}
#[get("/{id}")] async fn get(r:HttpRequest,a:Data<crate::auth::TelegramAuth>,i:Data<Arc<FileUserIdentityStore>>,s:Data<BookStore>,id:Path<String>)->impl Responder{let u=match actor(&r,&a,&i){Ok(v)=>v,Err(e)=>return e};match s.get(&u,&id).await{Ok(v)=>HttpResponse::Ok().json(v),Err(e)=>book_error(e)}}
#[put("/{id}")] async fn update(r:HttpRequest,a:Data<crate::auth::TelegramAuth>,i:Data<Arc<FileUserIdentityStore>>,s:Data<BookStore>,id:Path<String>,b:Json<BookUpdate>)->impl Responder{let u=match actor(&r,&a,&i){Ok(v)=>v,Err(e)=>return e};match s.update(&u,&id,b.into_inner()).await{Ok(v)=>HttpResponse::Ok().json(v),Err(e)=>book_error(e)}}
#[delete("/{id}")] async fn remove(r:HttpRequest,a:Data<crate::auth::TelegramAuth>,i:Data<Arc<FileUserIdentityStore>>,s:Data<BookStore>,id:Path<String>)->impl Responder{let u=match actor(&r,&a,&i){Ok(v)=>v,Err(e)=>return e};match s.remove(&u,&id).await{Ok(())=>HttpResponse::NoContent().finish(),Err(e)=>book_error(e)}}
pub fn books_api()->Scope{web::scope("/books").service(search_catalogue).service(catalogue_item).service(list).service(add).service(refresh_catalogue).service(holdings).service(get).service(update).service(remove)}
