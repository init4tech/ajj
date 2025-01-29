use crate::{
    types::{InboundData, Response},
    HandlerCtx, TaskSet,
};
use axum::{extract::FromRequest, response::IntoResponse};
use bytes::Bytes;
use std::{future::Future, pin::Pin};
use tokio::runtime::Handle;

/// A wrapper around an [`ajj::Router`] that implements the [`axum::handler::Handler`] trait.
#[derive(Debug, Clone)]
pub struct IntoAxum<S> {
    pub(crate) router: crate::Router<S>,
    pub(crate) task_set: TaskSet,
}

impl<S> From<crate::Router<S>> for IntoAxum<S> {
    fn from(router: crate::Router<S>) -> Self {
        Self {
            router,
            task_set: Default::default(),
        }
    }
}

impl<S> IntoAxum<S> {
    /// Create a new `IntoAxum` from a router and task set.
    pub(crate) fn new(router: crate::Router<S>, handle: Handle) -> Self {
        Self {
            router,
            task_set: handle.into(),
        }
    }

    /// Get a new context, built from the task set.
    fn ctx(&self) -> HandlerCtx {
        self.task_set.clone().into()
    }
}

impl<S> axum::handler::Handler<Bytes, S> for IntoAxum<S>
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
                .router
                .call_batch_with_state(self.ctx(), req, state)
                .await
            {
                Box::<str>::from(response).into_response()
            } else {
                ().into_response()
            }
        })
    }
}
