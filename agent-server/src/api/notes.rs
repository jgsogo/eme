//! Authenticated CRUD endpoints for personal and family-shared notes.

mod create_note_request;
mod update_note_request;

use std::sync::Arc;

use actix_web::{
    HttpRequest, HttpResponse, Responder, Scope, delete, get, post, put,
    web::{self, Data, Json, Path},
};
use agent_storage::{FileUserIdentityStore, UserIdentityStore};
use notes_tools::{NewNote, NotesStore, UpdateNote};

use self::{create_note_request::CreateNoteRequest, update_note_request::UpdateNoteRequest};

fn actor(
    request: &HttpRequest,
    auth: &crate::auth::TelegramAuth,
    identities: &FileUserIdentityStore,
) -> Result<String, HttpResponse> {
    let owner = auth
        .owner_for_request(request)
        .ok_or_else(|| HttpResponse::Unauthorized().finish())?;
    let subject = owner
        .strip_prefix("telegram:")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| HttpResponse::Unauthorized().finish())?;
    let identity = identities
        .find_identity("telegram", subject)
        .map_err(|_| HttpResponse::InternalServerError().finish())?
        .ok_or_else(|| HttpResponse::Unauthorized().finish())?;
    Ok(identity.user_id.to_string())
}

fn error_response(error: notes_tools::NotesError) -> HttpResponse {
    match error {
        notes_tools::NotesError::NotFound => HttpResponse::NotFound().finish(),
        notes_tools::NotesError::Invalid(message) => HttpResponse::BadRequest().body(message),
        notes_tools::NotesError::Family(_) | notes_tools::NotesError::Storage(_) => {
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[get("")]
async fn list_notes(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    store: Data<NotesStore>,
) -> impl Responder {
    let user = match actor(&request, &auth, &identities) {
        Ok(user) => user,
        Err(response) => return response,
    };
    match store.list_accessible(&user, None).await {
        Ok(notes) => HttpResponse::Ok().json(notes),
        Err(error) => error_response(error),
    }
}

#[post("")]
async fn create_note(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    store: Data<NotesStore>,
    body: Json<CreateNoteRequest>,
) -> impl Responder {
    let user = match actor(&request, &auth, &identities) {
        Ok(user) => user,
        Err(response) => return response,
    };
    match store
        .create(NewNote {
            owner_id: user,
            family_id: body.family_id.clone(),
            title: body.title.clone(),
            content: body.content.clone(),
        })
        .await
    {
        Ok(note) => HttpResponse::Created().json(note),
        Err(error) => error_response(error),
    }
}

#[get("/{note_id}")]
async fn get_note(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    store: Data<NotesStore>,
    note_id: Path<String>,
) -> impl Responder {
    let user = match actor(&request, &auth, &identities) {
        Ok(user) => user,
        Err(response) => return response,
    };
    match store.get_accessible(&user, None, &note_id).await {
        Ok(note) => HttpResponse::Ok().json(note),
        Err(error) => error_response(error),
    }
}

#[put("/{note_id}")]
async fn update_note(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    store: Data<NotesStore>,
    note_id: Path<String>,
    body: Json<UpdateNoteRequest>,
) -> impl Responder {
    let user = match actor(&request, &auth, &identities) {
        Ok(user) => user,
        Err(response) => return response,
    };
    match store
        .update_owned(
            &user,
            &note_id,
            UpdateNote {
                title: Some(body.title.clone()),
                content: Some(body.content.clone()),
                family_id: Some(body.family_id.clone()),
            },
        )
        .await
    {
        Ok(note) => HttpResponse::Ok().json(note),
        Err(error) => error_response(error),
    }
}

#[delete("/{note_id}")]
async fn delete_note(
    request: HttpRequest,
    auth: Data<crate::auth::TelegramAuth>,
    identities: Data<Arc<FileUserIdentityStore>>,
    store: Data<NotesStore>,
    note_id: Path<String>,
) -> impl Responder {
    let user = match actor(&request, &auth, &identities) {
        Ok(user) => user,
        Err(response) => return response,
    };
    match store.delete_owned(&user, &note_id).await {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(error) => error_response(error),
    }
}

pub fn notes_api() -> Scope {
    web::scope("/notes")
        .service(list_notes)
        .service(create_note)
        .service(get_note)
        .service(update_note)
        .service(delete_note)
}
