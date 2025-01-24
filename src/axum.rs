use crate::{
    types::{Request, Response},
    HandlerArgs,
};
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

            let Ok(req) = Request::try_from(bytes) else {
                return Box::<str>::from(Response::parse_error()).into_response();
            };

            let args = HandlerArgs {
                ctx: Default::default(),
                req,
            };

            // Default handler ctx does not allow for notifications, which is
            // what we want over HTTP.
            let response = unwrap_infallible!(self.call_with_state(args, state).await);

            Box::<str>::from(response).into_response()
        })
    }
}
