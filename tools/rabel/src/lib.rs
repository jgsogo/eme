//! Initial connectivity proof of concept for the RABEL / AbsysNet catalogue.
//!
//! The client follows AbsysNet's HTML meta-refresh into a session URL and can
//! fetch an individual bibliographic record. It deliberately returns raw HTML:
//! parsing bibliographic fields and copy-level availability is the next step.

mod client;
mod search_response;

pub use client::RabelClient;
pub use search_response::SearchResponse;
