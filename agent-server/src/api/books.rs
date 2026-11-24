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

#[derive(Deserialize)]
struct CatalogueItemQuery {
    q: String,
    title: Option<String>,
    isbn: Option<String>,
}

#[get("/catalogue/item")]
async fn catalogue_item(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    query: web::Query<CatalogueItemQuery>,
) -> impl Responder {
    if let Err(response) = actor(&request, &auth, &identities) { return response; }
    if query.q.trim().is_empty() {
        return HttpResponse::BadRequest().body("q must not be empty");
    }
    let client = match RabelClient::new() {
        Ok(client) => client,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };
    // Record references contain an AbsysNet session path. Never accept a
    // reference from a previous HTTP request: search and detail retrieval must
    // happen through this request's single cookie-preserving client.
    let results = match client.search(&query.q, 20).await {
        Ok(results) => results,
        Err(_) => return HttpResponse::BadGateway().body("Could not search RABEL for the selected record"),
    };
    let wanted_isbn = query.isbn.as_deref().filter(|value| !value.trim().is_empty()).map(normalize_isbn);
    let wanted_title = query.title.as_deref().unwrap_or(&query.q);
    let mut matches = Vec::new();
    for result in results {
        let item = match client.get_item(&result.record_ref).await {
            Ok(item) => item,
            Err(_) => continue,
        };
        let matched = if let Some(wanted) = &wanted_isbn {
            item.isbn.as_deref().or(result.isbn.as_deref())
                .is_some_and(|candidate| normalize_isbn(candidate) == *wanted)
        } else {
            item.title.as_deref().is_some_and(|title| {
                catalogue_title_matches(wanted_title, title)
                    && catalogue_title_matches(&result.title, title)
            })
        };
        if matched { matches.push(item); }
    }
    match matches.len() {
        0 => HttpResponse::NotFound().body("No matching RABEL record was found"),
        1 => HttpResponse::Ok().json(matches.remove(0)),
        _ => HttpResponse::Conflict().body("More than one RABEL record matches; provide an ISBN to identify the edition"),
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
    let mut matched_items = Vec::new();
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
            item.isbn.as_deref().or(result.isbn.as_deref())
                .is_some_and(|candidate| normalize_isbn(candidate) == *wanted)
        } else {
            item.title.as_deref().is_some_and(|title| catalogue_title_matches(&book.title, title))
        };
        if matches {
            matched_items.push(item);
        }
    }

    // If the stored ISBN is absent from RABEL's search index or stale, make
    // one conservative fallback pass and accept only an exact parsed-title match.
    if matched_items.is_empty() && wanted_isbn.is_some() {
        if let Ok(fallback_results) = client.search(&book.title, 20).await {
            for result in fallback_results {
                let item = match client.get_item(&result.record_ref).await {
                    Ok(item) => item,
                    Err(_) => continue,
                };
                if item.title.as_deref().is_some_and(|title| catalogue_title_matches(&book.title, title)) {
                    matched_items.push(item);
                }
            }
        }
    }

    let item = match matched_items.len() {
        0 => return HttpResponse::NotFound().body("No exact matching RABEL record was found; collection data was left unchanged"),
        1 => matched_items.pop().expect("one matched catalogue record"),
        _ => return HttpResponse::Conflict().body("Multiple RABEL records match this book; collection data was left unchanged"),
    };
    let metadata_update = BookUpdate {
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
    match store.update(&user, &book.id, metadata_update).await {
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
        let mut matching_records = wanted_isbn.as_ref().map(|isbn| {
            results.iter().filter(|result| {
                result.isbn.as_deref().is_some_and(|candidate| normalize_isbn(candidate) == *isbn)
            }).cloned().collect::<Vec<_>>()
        }).unwrap_or_default();
        if matching_records.is_empty() {
            // ISBN is preferred, but some catalogue records omit it. Fall back
            // only to exact normalized title matches, never the first hit.
            matching_records = results.iter()
                .filter(|result| catalogue_title_matches(&result.title, &book.title))
                .cloned().collect();
        }
        if matching_records.is_empty() {
            library_results.push(serde_json::json!({
                "library_id": library_id,
                "library_name": library_name,
                "status": "not_found",
                "records": []
            }));
            continue;
        }

        // Do not hide ambiguous editions: fetch the full bibliographic record
        // and copy-level holdings for every exact candidate in this library.
        let mut records = Vec::with_capacity(matching_records.len());
        for candidate in matching_records {
            let item = match client.get_item(&candidate.record_ref).await {
                Ok(item) => item,
                Err(error) => {
                    records.push(serde_json::json!({
                        "catalogue_title": candidate.title,
                        "isbn": candidate.isbn,
                        "status": "record_error",
                        "error": error.to_string(),
                        "copies": []
                    }));
                    continue;
                }
            };
            // Validate the parsed title too. A search result heading can
            // include author/responsibility text around the actual title.
            let title_matches = item.title.as_deref()
                .is_some_and(|title| catalogue_title_matches(&candidate.title, title)
                    && catalogue_title_matches(&book.title, title));
            let isbn_matches = wanted_isbn.as_ref().is_some_and(|wanted| {
                item.isbn.as_deref().or(candidate.isbn.as_deref())
                    .is_some_and(|isbn| normalize_isbn(isbn) == *wanted)
            });
            if !title_matches && !isbn_matches { continue; }

            match client.get_holdings(&candidate.record_ref).await {
                Ok(holdings) => records.push(serde_json::json!({
                    "catalogue_title": item.title,
                    "authors": item.authors,
                    "isbn": item.isbn,
                    "publisher": item.publisher,
                    "publication_year": item.publication_year,
                    "publication_statement": item.publication_statement,
                    "physical_description": item.physical_description,
                    "series": item.series,
                    "checked_at": holdings.checked_at,
                    "copies": holdings.copies
                })),
                Err(error) => records.push(serde_json::json!({
                    "catalogue_title": item.title,
                    "authors": item.authors,
                    "isbn": item.isbn,
                    "publisher": item.publisher,
                    "publication_year": item.publication_year,
                    "publication_statement": item.publication_statement,
                    "physical_description": item.physical_description,
                    "series": item.series,
                    "status": "holdings_error",
                    "error": error.to_string(),
                    "copies": []
                })),
            }
        }
        let status = match records.len() {
            0 => "not_found",
            1 => "found",
            _ => "ambiguous",
        };
        library_results.push(serde_json::json!({
            "library_id": library_id,
            "library_name": library_name,
            "status": status,
            "records": records
        }));
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

/// Search-result link text can prepend the author to the title and append a
/// responsibility statement. Match the parsed title only as a complete value
/// or as a suffix of that bibliographic prefix, never as an arbitrary substring.
fn catalogue_title_matches(search_title: &str, parsed_title: &str) -> bool {
    let search_prefix = search_title.split(" / ").next().unwrap_or(search_title);
    let search = normalize_title(search_prefix);
    let parsed = normalize_title(parsed_title);
    search == parsed || (parsed.split_whitespace().count() >= 2 && search.ends_with(&format!(" {parsed}")))
}

#[get("")] async fn list(r:HttpRequest,a:Data<crate::auth::TelegramAuth>,i:Data<Arc<FileUserIdentityStore>>,s:Data<BookStore>)->impl Responder{let u=match actor(&r,&a,&i){Ok(v)=>v,Err(e)=>return e};match s.list(&u).await{Ok(v)=>HttpResponse::Ok().json(v),Err(e)=>book_error(e)}}
#[post("")] async fn add(r:HttpRequest,a:Data<crate::auth::TelegramAuth>,i:Data<Arc<FileUserIdentityStore>>,s:Data<BookStore>,b:Json<BookInput>)->impl Responder{let u=match actor(&r,&a,&i){Ok(v)=>v,Err(e)=>return e};match s.add(&u,b.into_inner()).await{Ok(v)=>HttpResponse::Created().json(v),Err(e)=>book_error(e)}}
#[get("/{id}")] async fn get(r:HttpRequest,a:Data<crate::auth::TelegramAuth>,i:Data<Arc<FileUserIdentityStore>>,s:Data<BookStore>,id:Path<String>)->impl Responder{let u=match actor(&r,&a,&i){Ok(v)=>v,Err(e)=>return e};match s.get(&u,&id).await{Ok(v)=>HttpResponse::Ok().json(v),Err(e)=>book_error(e)}}
#[put("/{id}")] async fn update(r:HttpRequest,a:Data<crate::auth::TelegramAuth>,i:Data<Arc<FileUserIdentityStore>>,s:Data<BookStore>,id:Path<String>,b:Json<BookUpdate>)->impl Responder{let u=match actor(&r,&a,&i){Ok(v)=>v,Err(e)=>return e};match s.update(&u,&id,b.into_inner()).await{Ok(v)=>HttpResponse::Ok().json(v),Err(e)=>book_error(e)}}
#[delete("/{id}")] async fn remove(r:HttpRequest,a:Data<crate::auth::TelegramAuth>,i:Data<Arc<FileUserIdentityStore>>,s:Data<BookStore>,id:Path<String>)->impl Responder{let u=match actor(&r,&a,&i){Ok(v)=>v,Err(e)=>return e};match s.remove(&u,&id).await{Ok(())=>HttpResponse::NoContent().finish(),Err(e)=>book_error(e)}}
pub fn books_api()->Scope{web::scope("/books").service(search_catalogue).service(catalogue_item).service(list).service(add).service(refresh_catalogue).service(holdings).service(get).service(update).service(remove)}
