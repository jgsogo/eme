//! Agent tools for the authenticated user's personal book collection and RABEL.
use agent_runtime::{Tool, ToolContext, ToolDefinition, ToolError, ToolRegistry};
use async_trait::async_trait;
use rabel_tool::{CatalogueItem, RabelClient};
use serde_json::{Value, json};

use crate::{Book, BookInput, BookStore, BookUpdate};

pub fn register_tools(registry: &mut ToolRegistry, store: BookStore) -> Result<(), agent_config::ConfigError> {
    registry.register(def("books.list", "List books in the authenticated user's personal collection.", object(json!({}), &[])), List(store.clone()))?;
    registry.register(def("books.get", "Get a book from the authenticated user's personal collection.", object(json!({"id":{"type":"string","minLength":1}}), &["id"])), Get(store.clone()))?;
    registry.register(def("books.add", "Add a book to the authenticated user's personal collection.", book_input_schema()), Add(store.clone()))?;
    registry.register(def("books.update", "Update catalogue or personal metadata for a book in the authenticated user's collection.", book_update_schema()), Update(store.clone()))?;
    registry.register(def("books.remove", "Remove a book from the authenticated user's personal collection.", object(json!({"id":{"type":"string","minLength":1}}), &["id"])), Remove(store.clone()))?;

    registry.register(def(
        "books.list_libraries",
        "List RABEL libraries and identify which ones are selected as this user's preferred libraries.",
        object(json!({}), &[])),
        ListLibraries(store.clone()),
    )?;
    registry.register(def(
        "books.set_preferred_libraries",
        "Replace the authenticated user's preferred RABEL libraries. Use library IDs returned by books.list_libraries; an empty list clears the selection.",
        object(json!({"library_ids":{"type":"array","items":{"type":"string","minLength":1},"uniqueItems":true}}), &["library_ids"])),
        SetPreferredLibraries(store.clone()),
    )?;
    registry.register(def(
        "books.check_availability",
        "Fetch live copy-level availability for a book in every preferred library. This performs fresh RABEL catalogue requests and does not infer availability from stored metadata.",
        object(json!({"id":{"type":"string","minLength":1}}), &["id"])),
        CheckAvailability(store.clone()),
    )?;
    registry.register(def(
        "books.check_catalogue_availability",
        "Check a catalogue title or ISBN in every preferred RABEL library without first adding it to the personal collection. Returns fresh copy-level holdings per library.",
        object(json!({"title":{"type":"string","minLength":1},"isbn":{"type":["string","null"]}}), &["title"])),
        CheckCatalogueAvailability(store.clone()),
    )?;
    registry.register(def(
        "books.refresh_catalogue",
        "Refresh bibliographic metadata for a collection book from RABEL, matching by ISBN first and exact parsed title as a fallback. Personal rating, reading status, purchase intent and review are preserved.",
        object(json!({"id":{"type":"string","minLength":1}}), &["id"])),
        RefreshCatalogue(store),
    )?;
    Ok(())
}

fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn def(name: &str, description: &str, parameters: Value) -> ToolDefinition {
    ToolDefinition { name: name.into(), description: description.into(), parameters }
}
fn book_input_schema() -> Value {
    object(json!({
        "title":{"type":"string","minLength":1},
        "authors":{"type":"array","items":{"type":"string"}},
        "isbn":{"type":["string","null"]},"publisher":{"type":["string","null"]},
        "publication_year":{"type":["integer","null"]},"responsibility":{"type":["string","null"]},
        "publication_statement":{"type":["string","null"]},"physical_description":{"type":["string","null"]},
        "series":{"type":["string","null"]},"notes":{"type":"array","items":{"type":"string"}},
        "subjects":{"type":"array","items":{"type":"string"}},"catalogue_url":{"type":["string","null"]},
        "rating":{"type":["integer","null"],"minimum":1,"maximum":5},
        "reading_status":{"type":"string","enum":["pending","reading","read"]},
        "want_to_buy":{"type":"boolean"},"review":{"type":["string","null"]}
    }), &["title"])
}
fn book_update_schema() -> Value {
    let mut properties = book_input_schema()["properties"].clone();
    properties.as_object_mut().unwrap().insert("id".into(), json!({"type":"string","minLength":1}));
    object(properties, &["id"])
}
fn user(context: &ToolContext) -> Result<&str, ToolError> {
    context.user_id.as_deref().filter(|value| !value.trim().is_empty())
        .ok_or_else(|| execution("book collection requires an authenticated user context"))
}
fn execution(error: impl std::fmt::Display) -> ToolError { ToolError::Execution(error.to_string()) }
fn normalize_isbn(value: &str) -> String {
    value.chars().filter(|character| character.is_ascii_alphanumeric())
        .map(|character| character.to_ascii_uppercase()).collect()
}
fn normalize_title(value: &str) -> String {
    value.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}
fn title_matches(search_title: &str, parsed_title: &str) -> bool {
    let prefix = search_title.split(" / ").next().unwrap_or(search_title);
    let search = normalize_title(prefix);
    let parsed = normalize_title(parsed_title);
    search == parsed || (parsed.split_whitespace().count() >= 2 && search.ends_with(&format!(" {parsed}")))
}
async fn find_catalogue_record(client: &RabelClient, book: &Book) -> Result<Option<(String, CatalogueItem)>, ToolError> {
    find_catalogue_record_by_identity(client, book.isbn.as_deref(), &book.title).await
}
async fn find_catalogue_record_by_identity(
    client: &RabelClient,
    isbn: Option<&str>,
    title: &str,
) -> Result<Option<(String, CatalogueItem)>, ToolError> {
    let isbn_query = isbn.filter(|value| !value.trim().is_empty());
    let mut queries = Vec::new();
    if let Some(isbn) = isbn_query { queries.push((isbn.to_owned(), true)); }
    queries.push((title.to_owned(), false));
    let mut matches = Vec::new();
    for (query, isbn_lookup) in queries {
        let results = client.search(&query, 20).await.map_err(execution)?;
        for result in results {
            let item = match client.get_item(&result.record_ref).await {
                Ok(item) => item,
                Err(_) => continue,
            };
            let matched = if isbn_lookup {
                let wanted = normalize_isbn(isbn_query.unwrap_or_default());
                item.isbn.as_deref().or(result.isbn.as_deref())
                    .is_some_and(|candidate| normalize_isbn(candidate) == wanted)
            } else {
                item.title.as_deref().is_some_and(|parsed| title_matches(&result.title, parsed) && title_matches(title, parsed))
            };
            if matched { matches.push((result.record_ref, item)); }
        }
        if !matches.is_empty() { break; }
    }
    match matches.len() {
        0 => Ok(None),
        1 => Ok(matches.pop()),
        _ => Err(execution("Multiple RABEL records match this title/ISBN; narrow the lookup before using availability or refreshing metadata")),
    }
}

struct List(BookStore);
struct Get(BookStore);
struct Add(BookStore);
struct Update(BookStore);
struct Remove(BookStore);
struct ListLibraries(BookStore);
struct SetPreferredLibraries(BookStore);
struct CheckAvailability(BookStore);
struct CheckCatalogueAvailability(BookStore);
struct RefreshCatalogue(BookStore);

#[async_trait]
impl Tool for List {
    async fn execute(&self, context: &ToolContext, _: Value) -> Result<String, ToolError> {
        serde_json::to_string(&self.0.list(user(context)?).await.map_err(execution)?).map_err(execution)
    }
}
#[async_trait]
impl Tool for Get {
    async fn execute(&self, context: &ToolContext, args: Value) -> Result<String, ToolError> {
        let id = args["id"].as_str().filter(|value| !value.trim().is_empty()).ok_or_else(|| execution("id is required"))?;
        serde_json::to_string(&self.0.get(user(context)?, id).await.map_err(execution)?).map_err(execution)
    }
}
#[async_trait]
impl Tool for Add {
    async fn execute(&self, context: &ToolContext, args: Value) -> Result<String, ToolError> {
        let input: BookInput = serde_json::from_value(args).map_err(execution)?;
        serde_json::to_string(&self.0.add(user(context)?, input).await.map_err(execution)?).map_err(execution)
    }
}
#[async_trait]
impl Tool for Update {
    async fn execute(&self, context: &ToolContext, mut args: Value) -> Result<String, ToolError> {
        let object = args.as_object_mut().ok_or_else(|| execution("arguments must be an object"))?;
        let id = object.remove("id").and_then(|value| value.as_str().map(str::to_owned))
            .filter(|value| !value.trim().is_empty()).ok_or_else(|| execution("id is required"))?;
        let update: BookUpdate = serde_json::from_value(args).map_err(execution)?;
        serde_json::to_string(&self.0.update(user(context)?, &id, update).await.map_err(execution)?).map_err(execution)
    }
}
#[async_trait]
impl Tool for Remove {
    async fn execute(&self, context: &ToolContext, args: Value) -> Result<String, ToolError> {
        let id = args["id"].as_str().filter(|value| !value.trim().is_empty()).ok_or_else(|| execution("id is required"))?;
        self.0.remove(user(context)?, id).await.map_err(execution)?;
        Ok(json!({"removed":true}).to_string())
    }
}
#[async_trait]
impl Tool for ListLibraries {
    async fn execute(&self, context: &ToolContext, _: Value) -> Result<String, ToolError> {
        let user = user(context)?;
        let client = RabelClient::new().map_err(execution)?;
        let available = client.libraries().await.map_err(execution)?;
        let selected = self.0.preferred_libraries(user).await.map_err(execution)?;
        serde_json::to_string(&json!({"available":available,"selected":selected})).map_err(execution)
    }
}
#[async_trait]
impl Tool for SetPreferredLibraries {
    async fn execute(&self, context: &ToolContext, args: Value) -> Result<String, ToolError> {
        let user = user(context)?;
        let ids = args["library_ids"].as_array().ok_or_else(|| execution("library_ids must be an array"))?;
        let client = RabelClient::new().map_err(execution)?;
        let available = client.libraries().await.map_err(execution)?;
        let previous = self.0.preferred_libraries(user).await.map_err(execution)?;
        let mut selected = Vec::new();
        for id in ids {
            let id = id.as_str().filter(|value| !value.trim().is_empty()).ok_or_else(|| execution("library IDs must be non-empty strings"))?;
            if let Some(library) = available.iter().find(|library| library.id == id) {
                selected.push((library.id.clone(), library.name.clone()));
            } else if let Some(saved) = previous.iter().find(|(saved_id, _)| saved_id == id) {
                selected.push(saved.clone());
            } else {
                return Err(execution(format!("unknown RABEL library ID: {id}")));
            }
        }
        self.0.set_preferred_libraries(user, &selected).await.map_err(execution)?;
        serde_json::to_string(&json!({"selected":selected})).map_err(execution)
    }
}
#[async_trait]
impl Tool for CheckAvailability {
    async fn execute(&self, context: &ToolContext, args: Value) -> Result<String, ToolError> {
        let user = user(context)?;
        let id = args["id"].as_str().filter(|value| !value.trim().is_empty()).ok_or_else(|| execution("id is required"))?;
        let book = self.0.get(user, id).await.map_err(execution)?;
        let libraries = self.0.preferred_libraries(user).await.map_err(execution)?;
        let mut results = Vec::new();
        for (library_id, library_name) in libraries {
            let client = match RabelClient::for_subcatalog(&library_id) {
                Ok(client) => client,
                Err(error) => { results.push(json!({"library_id":library_id,"library_name":library_name,"status":"error","error":error.to_string(),"copies":[]})); continue; }
            };
            let (record_ref, record) = match find_catalogue_record(&client, &book).await {
                Ok(Some(record)) => record,
                Ok(None) => { results.push(json!({"library_id":library_id,"library_name":library_name,"status":"not_found","copies":[]})); continue; }
                Err(error) => { results.push(json!({"library_id":library_id,"library_name":library_name,"status":"error","error":error.to_string(),"copies":[]})); continue; }
            };
            match client.get_holdings(&record_ref).await {
                Ok(holdings) => results.push(json!({"library_id":library_id,"library_name":library_name,"status":"found","catalogue_title":record.title,"checked_at":holdings.checked_at,"copies":holdings.copies})),
                Err(error) => results.push(json!({"library_id":library_id,"library_name":library_name,"status":"error","error":error.to_string(),"copies":[]})),
            }
        }
        serde_json::to_string(&json!({"book_id":book.id,"libraries":results})).map_err(execution)
    }
}
#[async_trait]
impl Tool for CheckCatalogueAvailability {
    async fn execute(&self, context: &ToolContext, args: Value) -> Result<String, ToolError> {
        let user = user(context)?;
        let title = args["title"].as_str().filter(|value| !value.trim().is_empty()).ok_or_else(|| execution("title is required"))?;
        let isbn = args["isbn"].as_str().filter(|value| !value.trim().is_empty());
        let libraries = self.0.preferred_libraries(user).await.map_err(execution)?;
        let mut results = Vec::new();
        for (library_id, library_name) in libraries {
            let client = match RabelClient::for_subcatalog(&library_id) {
                Ok(client) => client,
                Err(error) => { results.push(json!({"library_id":library_id,"library_name":library_name,"status":"error","error":error.to_string(),"copies":[]})); continue; }
            };
            let (record_ref, record) = match find_catalogue_record_by_identity(&client, isbn, title).await {
                Ok(Some(record)) => record,
                Ok(None) => { results.push(json!({"library_id":library_id,"library_name":library_name,"status":"not_found","copies":[]})); continue; }
                Err(error) => { results.push(json!({"library_id":library_id,"library_name":library_name,"status":"error","error":error.to_string(),"copies":[]})); continue; }
            };
            match client.get_holdings(&record_ref).await {
                Ok(holdings) => results.push(json!({"library_id":library_id,"library_name":library_name,"status":"found","catalogue_title":record.title,"checked_at":holdings.checked_at,"copies":holdings.copies})),
                Err(error) => results.push(json!({"library_id":library_id,"library_name":library_name,"status":"error","error":error.to_string(),"copies":[]})),
            }
        }
        serde_json::to_string(&json!({"title":title,"isbn":isbn,"libraries":results})).map_err(execution)
    }
}

#[async_trait]
impl Tool for RefreshCatalogue {
    async fn execute(&self, context: &ToolContext, args: Value) -> Result<String, ToolError> {
        let user = user(context)?;
        let id = args["id"].as_str().filter(|value| !value.trim().is_empty()).ok_or_else(|| execution("id is required"))?;
        let book = self.0.get(user, id).await.map_err(execution)?;
        let client = RabelClient::new().map_err(execution)?;
        let (_, item) = find_catalogue_record(&client, &book).await?.ok_or_else(|| execution("No exact matching RABEL record was found; collection data was left unchanged"))?;
        let updated = self.0.update(user, id, BookUpdate {
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
        }).await.map_err(execution)?;
        serde_json::to_string(&json!({"book":updated,"catalogue_item":item})).map_err(execution)
    }
}
