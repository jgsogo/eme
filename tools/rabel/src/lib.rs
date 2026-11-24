//! Initial connectivity proof of concept for the RABEL / AbsysNet catalogue.
//!
//! This crate currently provides a small HTTP client that submits a catalogue
//! search and returns the raw HTML response. It is intentionally not registered
//! as an agent tool yet: the first goal is to verify the real OPAC endpoint,
//! query parameters, redirects, cookies, and response structure before writing
//! a parser against representative HTML fixtures.
//!
//! Bibliographic parsing, item availability, agent tool definitions, and server
//! integration will be added after this probe confirms the catalogue behaviour.

mod client;
mod search_response;

pub use client::RabelClient;
pub use search_response::SearchResponse;
