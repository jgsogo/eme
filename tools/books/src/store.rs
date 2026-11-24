use std::{path::Path, sync::Arc};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{Book, BookInput, BookStoreError, BookUpdate, ReadingStatus};

#[derive(Clone)]
pub struct BookStore {
    connection: Arc<Mutex<Connection>>,
}

impl BookStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, BookStoreError> {
        let connection = Connection::open(path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(include_str!("../migrations/0001_books.sql"))?;
        Ok(Self { connection: Arc::new(Mutex::new(connection)) })
    }

    pub async fn add(&self, user_id: &str, input: BookInput) -> Result<Book, BookStoreError> {
        validate_user(user_id)?;
        validate_input(&input)?;
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let authors = serde_json::to_string(&input.authors)?;
        let connection = self.connection.lock().await;
        connection.execute(
            "INSERT INTO user_books(id,user_id,title,authors_json,isbn,publisher,publication_year,catalogue_url,rating,reading_status,want_to_buy,review,created_at,updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?13)",
            params![id,user_id,input.title.trim(),authors,input.isbn,input.publisher,input.publication_year,input.catalogue_url,input.rating,input.reading_status.as_str(),input.want_to_buy as i64,input.review,now],
        )?;
        load(&connection, user_id, &id)?.ok_or(BookStoreError::NotFound)
    }

    pub async fn list(&self, user_id: &str) -> Result<Vec<Book>, BookStoreError> {
        validate_user(user_id)?;
        let connection = self.connection.lock().await;
        let mut statement = connection.prepare(
            "SELECT id,title,authors_json,isbn,publisher,publication_year,catalogue_url,rating,reading_status,want_to_buy,review,created_at,updated_at
             FROM user_books WHERE user_id=?1 ORDER BY updated_at DESC,id"
        )?;
        let rows = statement.query_map([user_id], row_to_book)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(BookStoreError::Storage)
    }

    pub async fn get(&self, user_id: &str, id: &str) -> Result<Book, BookStoreError> {
        validate_user(user_id)?;
        let connection = self.connection.lock().await;
        load(&connection, user_id, id)?.ok_or(BookStoreError::NotFound)
    }

    pub async fn update(&self, user_id: &str, id: &str, update: BookUpdate) -> Result<Book, BookStoreError> {
        validate_user(user_id)?;
        let mut book = self.get(user_id, id).await?;
        if let Some(title) = update.title { if title.trim().is_empty() { return Err(BookStoreError::Invalid("title must not be empty".into())); } book.title=title.trim().to_owned(); }
        if let Some(authors)=update.authors { book.authors=authors; }
        if let Some(isbn)=update.isbn { book.isbn=isbn; }
        if let Some(publisher)=update.publisher { book.publisher=publisher; }
        if let Some(year)=update.publication_year { book.publication_year=year; }
        if let Some(url)=update.catalogue_url { book.catalogue_url=url; }
        if let Some(rating)=update.rating { book.rating=rating; }
        if let Some(status)=update.reading_status { book.reading_status=status; }
        if let Some(want_to_buy)=update.want_to_buy { book.want_to_buy=want_to_buy; }
        if let Some(review)=update.review { book.review=review; }
        validate_rating(book.rating)?;
        let now=Utc::now();
        let authors=serde_json::to_string(&book.authors)?;
        let connection=self.connection.lock().await;
        connection.execute(
            "UPDATE user_books SET title=?1,authors_json=?2,isbn=?3,publisher=?4,publication_year=?5,catalogue_url=?6,rating=?7,reading_status=?8,want_to_buy=?9,review=?10,updated_at=?11 WHERE id=?12 AND user_id=?13",
            params![book.title,authors,book.isbn,book.publisher,book.publication_year,book.catalogue_url,book.rating,book.reading_status.as_str(),book.want_to_buy as i64,book.review,now.to_rfc3339(),id,user_id],
        )?;
        load(&connection,user_id,id)?.ok_or(BookStoreError::NotFound)
    }

    pub async fn remove(&self, user_id: &str, id: &str) -> Result<(), BookStoreError> {
        validate_user(user_id)?;
        let connection=self.connection.lock().await;
        let changed=connection.execute("DELETE FROM user_books WHERE id=?1 AND user_id=?2",params![id,user_id])?;
        if changed==0 { return Err(BookStoreError::NotFound); }
        Ok(())
    }

    pub async fn set_preferred_libraries(&self, user_id: &str, libraries: &[(String,String)]) -> Result<(), BookStoreError> {
        validate_user(user_id)?;
        let mut seen=std::collections::HashSet::new();
        for (id,name) in libraries {
            if id.trim().is_empty() || name.trim().is_empty() { return Err(BookStoreError::Invalid("library id and name must not be empty".into())); }
            if !seen.insert(id) { return Err(BookStoreError::Invalid(format!("duplicate library id: {id}"))); }
        }
        let mut connection=self.connection.lock().await;
        let tx=connection.transaction()?;
        tx.execute("DELETE FROM user_library_preferences WHERE user_id=?1",[user_id])?;
        for (id,name) in libraries {
            tx.execute("INSERT INTO user_library_preferences(user_id,library_id,library_name) VALUES (?1,?2,?3)",params![user_id,id,name])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub async fn preferred_libraries(&self, user_id: &str) -> Result<Vec<(String,String)>, BookStoreError> {
        validate_user(user_id)?;
        let connection=self.connection.lock().await;
        let mut statement=connection.prepare("SELECT library_id,library_name FROM user_library_preferences WHERE user_id=?1 ORDER BY library_name,library_id")?;
        let rows=statement.query_map([user_id],|row|Ok((row.get(0)?,row.get(1)?)))?;
        rows.collect::<Result<Vec<_>,_>>().map_err(BookStoreError::Storage)
    }
}

fn validate_user(user: &str) -> Result<(), BookStoreError> {
    if user.trim().is_empty() { Err(BookStoreError::Invalid("authenticated user id is required".into())) } else { Ok(()) }
}
fn validate_input(input: &BookInput) -> Result<(), BookStoreError> {
    if input.title.trim().is_empty() { return Err(BookStoreError::Invalid("title must not be empty".into())); }
    validate_rating(input.rating)
}
fn validate_rating(rating: Option<u8>) -> Result<(), BookStoreError> {
    if rating.is_some_and(|value| !(1..=5).contains(&value)) { Err(BookStoreError::Invalid("rating must be between 1 and 5".into())) } else { Ok(()) }
}
fn load(connection: &Connection,user:&str,id:&str)->Result<Option<Book>,BookStoreError>{
    connection.query_row(
        "SELECT id,title,authors_json,isbn,publisher,publication_year,catalogue_url,rating,reading_status,want_to_buy,review,created_at,updated_at FROM user_books WHERE user_id=?1 AND id=?2",
        params![user,id],row_to_book
    ).optional().map_err(BookStoreError::Storage)
}
fn row_to_book(row:&rusqlite::Row<'_>)->Result<Book,rusqlite::Error>{
    let authors:String=row.get(2)?;
    let status:String=row.get(8)?;
    let created:String=row.get(11)?;
    let updated:String=row.get(12)?;
    let parse_time=|value:&str| DateTime::parse_from_rfc3339(value).map(|t|t.with_timezone(&Utc)).map_err(|e|rusqlite::Error::FromSqlConversionFailure(0,rusqlite::types::Type::Text,Box::new(e)));
    Ok(Book{
        id:row.get(0)?,title:row.get(1)?,
        authors:serde_json::from_str(&authors).map_err(|e|rusqlite::Error::FromSqlConversionFailure(2,rusqlite::types::Type::Text,Box::new(e)))?,
        isbn:row.get(3)?,publisher:row.get(4)?,publication_year:row.get(5)?,catalogue_url:row.get(6)?,rating:row.get(7)?,
        reading_status:ReadingStatus::try_from(status.as_str()).map_err(|e|rusqlite::Error::FromSqlConversionFailure(8,rusqlite::types::Type::Text,Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData,e))))?,
        want_to_buy:row.get::<_,i64>(9)?!=0,review:row.get(10)?,
        created_at:parse_time(&created)?,updated_at:parse_time(&updated)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs};

    fn temp_db() -> (std::path::PathBuf, String) {
        let path = env::temp_dir().join(format!("eme-books-{}.sqlite", Uuid::new_v4()));
        (path.clone(), path.to_string_lossy().into_owned())
    }

    fn input(title:&str)->BookInput{
        BookInput{title:title.into(),authors:vec!["Miguel de Cervantes".into()],isbn:None,publisher:None,publication_year:None,catalogue_url:None,rating:Some(5),reading_status:ReadingStatus::Pending,want_to_buy:false,review:None}
    }

    #[tokio::test]
    async fn books_are_isolated_by_user_and_support_crud(){
        let (path, _) = temp_db();
        let store=BookStore::open(&path).unwrap();
        let book=store.add("user-a",input("Don Quijote")).await.unwrap();
        assert_eq!(store.list("user-a").await.unwrap().len(),1);
        assert!(store.get("user-b",&book.id).await.is_err());
        assert!(store.remove("user-b",&book.id).await.is_err());
        store.update("user-a",&book.id,BookUpdate{reading_status:Some(ReadingStatus::Read),..Default::default()}).await.unwrap();
        assert_eq!(store.get("user-a",&book.id).await.unwrap().reading_status,ReadingStatus::Read);
        store.remove("user-a",&book.id).await.unwrap();
        assert!(store.list("user-a").await.unwrap().is_empty());
        drop(store);
        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn library_preferences_are_scoped_to_user(){
        let (path, _) = temp_db();
        let store=BookStore::open(&path).unwrap();
        store.set_preferred_libraries("user-a",&[("SAL/BPSA".into(),"Biblioteca Pública de Salamanca".into())]).await.unwrap();
        assert_eq!(store.preferred_libraries("user-a").await.unwrap().len(),1);
        assert!(store.preferred_libraries("user-b").await.unwrap().is_empty());
        drop(store);
        let _ = fs::remove_file(path);
    }
}
