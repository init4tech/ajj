use crate::types::{InboundData, Response};
use axum::{extract::FromRequest, response::IntoResponse};
use bytes::Bytes;
use std::{future::Future, pin::Pin};

impl<S> axum::handler::Handler<Bytes, S> for crate::Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    type Future = Pin<Box<dyn Future<Output = axum::response::Response> + Send>>;

    fn call(self, req: axum::extract::Request, state: S) -> Self::Future {
        Box::pin(async move {
            let Ok(bytes) = Bytes::from_request(req, &state).await else {
                return Box::<str>::from(Response::parse_error()).into_response();
            };

            // If the inbound data is not currently parsable, we
            // send an empty one it to the router, as the router enforces
            // the specification.
            let req = InboundData::try_from(bytes).unwrap_or_default();

            if let Some(response) = self
                .call_batch_with_state(Default::default(), req, state)
                .await
            {
                Box::<str>::from(response).into_response()
            } else {
                ().into_response()
            }
        })
    }
}
