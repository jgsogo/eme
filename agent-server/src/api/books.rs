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
#[get("")] async fn list(r:HttpRequest,a:Data<crate::auth::TelegramAuth>,i:Data<Arc<FileUserIdentityStore>>,s:Data<BookStore>)->impl Responder{let u=match actor(&r,&a,&i){Ok(v)=>v,Err(e)=>return e};match s.list(&u).await{Ok(v)=>HttpResponse::Ok().json(v),Err(e)=>error(e)}}
#[post("")] async fn add(r:HttpRequest,a:Data<crate::auth::TelegramAuth>,i:Data<Arc<FileUserIdentityStore>>,s:Data<BookStore>,b:Json<BookInput>)->impl Responder{let u=match actor(&r,&a,&i){Ok(v)=>v,Err(e)=>return e};match s.add(&u,b.into_inner()).await{Ok(v)=>HttpResponse::Created().json(v),Err(e)=>error(e)}}
#[get("/libraries")] async fn libraries(r:HttpRequest,a:Data<crate::auth::TelegramAuth>,i:Data<Arc<FileUserIdentityStore>>,s:Data<BookStore>)->impl Responder{let u=match actor(&r,&a,&i){Ok(v)=>v,Err(e)=>return e};let c=match RabelClient::new(){Ok(v)=>v,Err(_)=>return HttpResponse::InternalServerError().finish()};let available=match c.libraries().await{Ok(v)=>v,Err(_)=>return HttpResponse::BadGateway().finish()};let selected=match s.preferred_libraries(&u).await{Ok(v)=>v,Err(e)=>return error(e)};HttpResponse::Ok().json(serde_json::json!({"available":available,"selected":selected}))}
#[put("/libraries")] async fn set_libraries(r:HttpRequest,a:Data<crate::auth::TelegramAuth>,i:Data<Arc<FileUserIdentityStore>>,s:Data<BookStore>,b:Json<LibrarySelection>)->impl Responder{let u=match actor(&r,&a,&i){Ok(v)=>v,Err(e)=>return e};let c=match RabelClient::new(){Ok(v)=>v,Err(_)=>return HttpResponse::InternalServerError().finish()};let available=match c.libraries().await{Ok(v)=>v,Err(_)=>return HttpResponse::BadGateway().finish()};let mut selected=Vec::new();for id in &b.library_ids{if let Some(l)=available.iter().find(|l|&l.id==id){selected.push((l.id.clone(),l.name.clone()));}else{return HttpResponse::BadRequest().body(format!("unknown RABEL library id: {id}"));}}match s.set_preferred_libraries(&u,&selected).await{Ok(())=>HttpResponse::Ok().json(serde_json::json!({"selected":selected})),Err(e)=>error(e)}}
#[get("/{id}")] async fn get(r:HttpRequest,a:Data<crate::auth::TelegramAuth>,i:Data<Arc<FileUserIdentityStore>>,s:Data<BookStore>,id:Path<String>)->impl Responder{let u=match actor(&r,&a,&i){Ok(v)=>v,Err(e)=>return e};match s.get(&u,&id).await{Ok(v)=>HttpResponse::Ok().json(v),Err(e)=>error(e)}}
#[put("/{id}")] async fn update(r:HttpRequest,a:Data<crate::auth::TelegramAuth>,i:Data<Arc<FileUserIdentityStore>>,s:Data<BookStore>,id:Path<String>,b:Json<BookUpdate>)->impl Responder{let u=match actor(&r,&a,&i){Ok(v)=>v,Err(e)=>return e};match s.update(&u,&id,b.into_inner()).await{Ok(v)=>HttpResponse::Ok().json(v),Err(e)=>error(e)}}
#[delete("/{id}")] async fn remove(r:HttpRequest,a:Data<crate::auth::TelegramAuth>,i:Data<Arc<FileUserIdentityStore>>,s:Data<BookStore>,id:Path<String>)->impl Responder{let u=match actor(&r,&a,&i){Ok(v)=>v,Err(e)=>return e};match s.remove(&u,&id).await{Ok(())=>HttpResponse::NoContent().finish(),Err(e)=>error(e)}}
pub fn books_api()->Scope{web::scope("/books").service(libraries).service(set_libraries).service(list).service(add).service(get).service(update).service(remove)}
