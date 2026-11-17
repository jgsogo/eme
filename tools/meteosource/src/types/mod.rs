//! Meteosource API models, client state and integration errors.

mod meteosource_error;
pub(crate) use meteosource_error::MeteosourceError;
mod place;
pub(crate) use place::Place;
mod meteosource_client;
pub(crate) use meteosource_client::MeteosourceClient;
