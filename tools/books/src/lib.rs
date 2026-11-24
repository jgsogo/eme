//! User-owned book collection and preferred-library persistence.
//!
//! This crate deliberately keeps personal collection data independent from
//! RABEL availability. Catalogue holdings are fetched live by rabel-tool;
//! ratings, reading state, purchase intent and reviews belong to EME users.

mod error;
mod model;
#[rustfmt::skip]
mod store;

pub use error::BookStoreError;
pub use model::{Book, BookInput, BookUpdate, ReadingStatus};
pub use store::BookStore;
