//! Core types, like [`Request`] and [`Response`].

#[macro_use]
mod macros;

mod batch;
pub(crate) use batch::InboundData;

mod req;
pub(crate) use req::Request;

mod resp;
pub(crate) use resp::Response;
pub use resp::{ErrorPayload, ResponsePayload};

mod error;
pub(crate) use error::RequestError;

pub(crate) const ID_LEN_LIMIT: usize = 80;
pub(crate) const METHOD_LEN_LIMIT: usize = 80;
