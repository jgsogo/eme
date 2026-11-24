//! Authenticated CRUD API for the current user's personal books and library preferences.
use std::sync::Arc;
use actix_web::{HttpRequest,HttpResponse,Responder,Scope,delete,get,post,put,web::{self,Data,Json,Path}};
use agent_storage::{FileUserIdentityStore,UserIdentityStore};
use books_tool::{BookInput,BookStore,BookStoreError,BookUpdate};
use rabel_tool::RabelClient;
use serde::Deserialize;
fn actor(r:&HttpRequest,a:&crate::auth::TelegramAuth,i:&FileUserIdentityStore)->Result<String,HttpResponse>{
 let owner=a.owner_for_request(r).ok_or_else(||HttpResponse::Unauthorized().finish())?;
 let subject=owner.strip_prefix("telegram:").filter(|v|!v.is_empty()).ok_or_else(||HttpResponse::Unauthorized().finish())?;
 let identity=i.find_identity("telegram",subject).map_err(|_|HttpResponse::InternalServerError().finish())?.ok_or_else(||HttpResponse::Unauthorized().finish())?;
 Ok(identity.user_id.to_string())
}
fn error(e:BookStoreError)->HttpResponse{match e{BookStoreError::NotFound=>HttpResponse::NotFound().finish(),BookStoreError::Invalid(m)=>HttpResponse::BadRequest().body(m),_=>HttpResponse::InternalServerError().finish()}}
#[derive(Deserialize)] struct LibrarySelection{library_ids:Vec<String>}
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
        Err(_) => HttpResponse::BadGateway().finish(),
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
        Err(error) => return error(error),
    };
    let selected = match store.preferred_libraries(&user).await {
        Ok(libraries) => libraries,
        Err(error) => return error(error),
    };
    let mut libraries = Vec::with_capacity(selected.len());
    for (library_id, library_name) in selected {
        let client = match RabelClient::for_subcatalog(&library_id) {
            Ok(client) => client,
            Err(error) => {
                libraries.push(serde_json::json!({
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
                libraries.push(serde_json::json!({
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
            libraries.push(serde_json::json!({
                "library_id": library_id,
                "library_name": library_name,
                "status": "not_found",
                "copies": []
            }));
            continue;
        };
        match client.get_item(&record.record_ref).await {
            Ok(item) => libraries.push(serde_json::json!({
                "library_id": library_id,
                "library_name": library_name,
                "status": "found",
                "catalogue_title": item.title,
                "checked_at": item.checked_at,
                "copies": item.holdings
            })),
            Err(error) => libraries.push(serde_json::json!({
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
        "libraries": libraries
    }))
}

fn normalize_isbn(value: &str) -> String {
    value.chars().filter(|character| character.is_ascii_alphanumeric())
        .map(|character| character.to_ascii_uppercase()).collect()
}

fn normalize_title(value: &str) -> String {
    value.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}

#[get("")] async fn list(r:HttpRequest,a:Data<crate::auth::TelegramAuth>,i:Data<Arc<FileUserIdentityStore>>,s:Data<BookStore>)->impl Responder{let u=match actor(&r,&a,&i){Ok(v)=>v,Err(e)=>return e};match s.list(&u).await{Ok(v)=>HttpResponse::Ok().json(v),Err(e)=>error(e)}}
#[post("")] async fn add(r:HttpRequest,a:Data<crate::auth::TelegramAuth>,i:Data<Arc<FileUserIdentityStore>>,s:Data<BookStore>,b:Json<BookInput>)->impl Responder{let u=match actor(&r,&a,&i){Ok(v)=>v,Err(e)=>return e};match s.add(&u,b.into_inner()).await{Ok(v)=>HttpResponse::Created().json(v),Err(e)=>error(e)}}
#[get("/libraries")] async fn libraries(r:HttpRequest,a:Data<crate::auth::TelegramAuth>,i:Data<Arc<FileUserIdentityStore>>,s:Data<BookStore>)->impl Responder{let u=match actor(&r,&a,&i){Ok(v)=>v,Err(e)=>return e};let c=match RabelClient::new(){Ok(v)=>v,Err(_)=>return HttpResponse::InternalServerError().finish()};let available=match c.libraries().await{Ok(v)=>v,Err(_)=>return HttpResponse::BadGateway().finish()};let selected=match s.preferred_libraries(&u).await{Ok(v)=>v,Err(e)=>return error(e)};HttpResponse::Ok().json(serde_json::json!({"available":available,"selected":selected}))}
#[put("/libraries")] async fn set_libraries(r:HttpRequest,a:Data<crate::auth::TelegramAuth>,i:Data<Arc<FileUserIdentityStore>>,s:Data<BookStore>,b:Json<LibrarySelection>)->impl Responder{let u=match actor(&r,&a,&i){Ok(v)=>v,Err(e)=>return e};let c=match RabelClient::new(){Ok(v)=>v,Err(_)=>return HttpResponse::InternalServerError().finish()};let available=match c.libraries().await{Ok(v)=>v,Err(_)=>return HttpResponse::BadGateway().finish()};let mut selected=Vec::new();for id in &b.library_ids{if let Some(l)=available.iter().find(|l|&l.id==id){selected.push((l.id.clone(),l.name.clone()));}else{return HttpResponse::BadRequest().body(format!("unknown RABEL library id: {id}"));}}match s.set_preferred_libraries(&u,&selected).await{Ok(())=>HttpResponse::Ok().json(serde_json::json!({"selected":selected})),Err(e)=>error(e)}}
#[get("/{id}")] async fn get(r:HttpRequest,a:Data<crate::auth::TelegramAuth>,i:Data<Arc<FileUserIdentityStore>>,s:Data<BookStore>,id:Path<String>)->impl Responder{let u=match actor(&r,&a,&i){Ok(v)=>v,Err(e)=>return e};match s.get(&u,&id).await{Ok(v)=>HttpResponse::Ok().json(v),Err(e)=>error(e)}}
#[put("/{id}")] async fn update(r:HttpRequest,a:Data<crate::auth::TelegramAuth>,i:Data<Arc<FileUserIdentityStore>>,s:Data<BookStore>,id:Path<String>,b:Json<BookUpdate>)->impl Responder{let u=match actor(&r,&a,&i){Ok(v)=>v,Err(e)=>return e};match s.update(&u,&id,b.into_inner()).await{Ok(v)=>HttpResponse::Ok().json(v),Err(e)=>error(e)}}
#[delete("/{id}")] async fn remove(r:HttpRequest,a:Data<crate::auth::TelegramAuth>,i:Data<Arc<FileUserIdentityStore>>,s:Data<BookStore>,id:Path<String>)->impl Responder{let u=match actor(&r,&a,&i){Ok(v)=>v,Err(e)=>return e};match s.remove(&u,&id).await{Ok(())=>HttpResponse::NoContent().finish(),Err(e)=>error(e)}}
pub fn books_api()->Scope{web::scope("/books").service(search_catalogue).service(libraries).service(set_libraries).service(list).service(add).service(holdings).service(get).service(update).service(remove)}
