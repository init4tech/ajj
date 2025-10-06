use crate::{
    types::{InboundData, Response},
    HandlerCtx, TaskSet, TracingInfo,
};
use axum::{
    extract::FromRequest,
    http::{header, HeaderValue},
    response::IntoResponse,
};
use bytes::Bytes;
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
};
use tokio::runtime::Handle;
use tracing::{debug, debug_span};
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// A wrapper around an [`Router`] that implements the
/// [`axum::handler::Handler`] trait. This struct is an implementation detail
/// of the [`Router::into_axum`] and [`Router::into_axum_with_handle`] methods.
///
/// [`Router`]: crate::Router
/// [`Router::into_axum`]: crate::Router::into_axum
/// [`Router::into_axum_with_handle`]: crate::Router::into_axum_with_handle
#[derive(Debug, Clone)]
pub(crate) struct IntoAxum<S> {
    pub(crate) router: crate::Router<S>,

    pub(crate) task_set: TaskSet,

    /// Counter for OTEL messages received.
    pub(crate) rx_msg_id: Arc<AtomicU32>,
    /// Counter for OTEL messages sent.
    pub(crate) tx_msg_id: Arc<AtomicU32>,
}

impl<S> From<crate::Router<S>> for IntoAxum<S> {
    fn from(router: crate::Router<S>) -> Self {
        Self {
            router,
            task_set: Default::default(),
            rx_msg_id: Arc::new(AtomicU32::new(0)),
            tx_msg_id: Arc::new(AtomicU32::new(0)),
        }
    }
}

impl<S> IntoAxum<S> {
    /// Create a new `IntoAxum` from a router and task set.
    pub(crate) fn new(router: crate::Router<S>, handle: Handle) -> Self {
        Self {
            router,
            task_set: handle.into(),
            rx_msg_id: Arc::new(AtomicU32::new(0)),
            tx_msg_id: Arc::new(AtomicU32::new(0)),
        }
    }
}

impl<S> IntoAxum<S>
where
    S: Clone + Send + Sync + 'static,
{
    fn ctx(&self) -> HandlerCtx {
        // This span is populated with as much detail as possible, and then
        // given to the Handler ctx. It will be populated with request-specific
        // details (e.g. method) during ctx instantiation.
        let request_span = debug_span!(
            // We could erase the parent here, however, axum or tower layers
            // may be creating per-request spans that we want to be children of.
            "ajj.IntoAxum::call",
            "otel.kind" = "server",
            "rpc.system" = "jsonrpc",
            "rpc.jsonrpc.version" = "2.0",
            "rpc.service" = self.router.service_name(),
            notifications_enabled = false,
            "otel.name" = tracing::field::Empty,
            "rpc.jsonrpc.request_id" = tracing::field::Empty,
            "rpc.jsonrpc.error_code" = tracing::field::Empty,
            "rpc.jsonrpc.error_message" = tracing::field::Empty,
            "rpc.method" = tracing::field::Empty,
            params = tracing::field::Empty
        );

        HandlerCtx::new(
            None,
            self.task_set.clone(),
            TracingInfo {
                service: self.router.service_name(),
                request_span,
            },
        )
    }
}

impl<S> axum::handler::Handler<Bytes, S> for IntoAxum<S>
where
    S: Clone + Send + Sync + 'static,
{
    type Future = Pin<Box<dyn Future<Output = axum::response::Response> + Send>>;

    fn call(self, req: axum::extract::Request, state: S) -> Self::Future {
        Box::pin(async move {
            let ctx = self.ctx();

            let parent_context = opentelemetry::global::get_text_map_propagator(|propagator| {
                propagator.extract(&opentelemetry_http::HeaderExtractor(req.headers()))
            });
            ctx.span().set_parent(parent_context).unwrap();

            let Ok(bytes) = Bytes::from_request(req, &state).await else {
                return Box::<str>::from(Response::parse_error()).into_response();
            };

            // https://github.com/open-telemetry/semantic-conventions/blob/main/docs/rpc/rpc-spans.md#message-event
            let req = ctx.span().in_scope(|| {
                //// https://github.com/open-telemetry/semantic-conventions/blob/d66109ff41e75f49587114e5bff9d101b87f40bd/docs/rpc/rpc-spans.md#events
                debug!(
                    "rpc.message.id" = self.rx_msg_id.fetch_add(1, Ordering::Relaxed),
                    "rpc.message.type" = "RECEIVED",
                    "rpc.message.uncompressed_size" = bytes.len(),
                    "rpc.message"
                );

                // If the inbound data is not currently parsable, we
                // send an empty one it to the router, as the router enforces
                // the specification.
                InboundData::try_from(bytes).unwrap_or_default()
            });

            let span = ctx.span().clone();
            if let Some(response) = self.router.call_batch_with_state(ctx, req, state).await {
                let headers = [(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static(mime::APPLICATION_JSON.as_ref()),
                )];

                let body = Box::<str>::from(response);

                span.in_scope(|| {
                    // https://github.com/open-telemetry/semantic-conventions/blob/d66109ff41e75f49587114e5bff9d101b87f40bd/docs/rpc/rpc-spans.md#events
                    debug!(
                        "rpc.message.id" = self.tx_msg_id.fetch_add(1, Ordering::Relaxed),
                        "rpc.message.type" = "SENT",
                        "rpc.message.uncompressed_size" = body.len(),
                    );
                });

                (headers, body).into_response()
            } else {
                ().into_response()
            }
        })
    }
}
