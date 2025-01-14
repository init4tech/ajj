use alloy::rpc::json_rpc::{Id, PartiallySerializedRequest, Response};
use axum::{extract::FromRequest, response::IntoResponse, Json};
use std::{future::Future, pin::Pin};

impl<S> axum::handler::Handler<Json<PartiallySerializedRequest>, S> for crate::Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    type Future = Pin<Box<dyn Future<Output = axum::response::Response> + Send>>;

    fn call(self, req: axum::extract::Request, state: S) -> Self::Future {
        Box::pin(async move {
            let json = Json::<PartiallySerializedRequest>::from_request(req, &state).await;
            let json = match json {
                Ok(Json(json)) => json,
                // rejections are JSON parse errors
                Err(err) => {
                    tracing::warn!(%err, "json extraction error");
                    return Json(Response::<(), ()>::parse_error(Id::None)).into_response();
                }
            };

            // Default handler ctx does not allow for notifications, which is
            // what we want over HTTP.
            let response =
                unwrap_infallible!(self.call_with_state(Default::default(), json, state).await);

            Json(response).into_response()
        })
    }
}
