mod ctx;
pub use ctx::{HandlerArgs, HandlerCtx, NotifyError, NotifyPermit, OwnedNotifyPermit, TracingInfo};

mod erased;
pub(crate) use erased::{BoxedIntoRoute, ErasedIntoRoute, MakeErasedHandler};

mod future;
pub use future::{BatchFuture, RouteFuture};

mod handler;
pub(crate) use handler::HandlerInternal;
pub use handler::{Handler, Params, State};

mod method;
pub(crate) use method::Method;

use crate::types::Response;
use serde_json::value::RawValue;
use std::{
    convert::Infallible,
    task::{Context, Poll},
};
use tower::{util::BoxCloneSyncService, Service, ServiceExt};
use tracing::{debug_span, enabled, Level};

/// A JSON-RPC handler for a specific method.
///
/// A route is a [`BoxCloneSyncService`] that takes JSON parameters and may
/// return a boxed [`RawValue`]. Routes SHOULD be infallible. I.e. any error
/// that occurs during the handling of a request should be represented as a
/// JSON-RPC error response, rather than having the service return an `Err`.
#[derive(Debug)]
pub(crate) struct Route(
    tower::util::BoxCloneSyncService<HandlerArgs, Option<Box<RawValue>>, Infallible>,
);

impl Route {
    /// Create a new route from a service.
    pub(crate) fn new<S>(inner: S) -> Self
    where
        S: Service<HandlerArgs, Response = Option<Box<RawValue>>, Error = Infallible>
            + Clone
            + Send
            + Sync
            + 'static,
        S::Future: Send + 'static,
    {
        Self(BoxCloneSyncService::new(inner))
    }

    /// Create a default fallback route that returns a method not found error.
    pub(crate) fn default_fallback() -> Self {
        Self::new(tower::service_fn(|args: HandlerArgs| async {
            let id = args.id_owned();
            crate::metrics::record_method_not_found(
                id.is_some(),
                args.service_name(),
                args.method(),
            );
            drop(args); // no longer needed
            Ok(Response::maybe_method_not_found(id.as_deref()))
        }))
    }

    /// Create a one-shot future for the given request.
    pub(crate) fn oneshot_inner(&mut self, args: HandlerArgs) -> RouteFuture {
        RouteFuture::new(self.0.clone().oneshot(args))
    }

    /// Variant of [`Route::oneshot_inner`] that takes ownership of the route to avoid cloning.
    pub(crate) fn oneshot_inner_owned(self, args: HandlerArgs) -> RouteFuture {
        RouteFuture::new(self.0.oneshot(args))
    }
}

impl Clone for Route {
    #[track_caller]
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl From<BoxCloneSyncService<HandlerArgs, Option<Box<RawValue>>, Infallible>> for Route {
    fn from(inner: BoxCloneSyncService<HandlerArgs, Option<Box<RawValue>>, Infallible>) -> Self {
        Self(inner)
    }
}

impl Service<HandlerArgs> for Route {
    type Response = Option<Box<RawValue>>;

    type Error = Infallible;

    type Future = RouteFuture;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, args: HandlerArgs) -> Self::Future {
        let span = debug_span!(
            "Route::call",
            notifications_enabled = args.ctx().notifications_enabled(),
            params = tracing::field::Empty,
        );
        if enabled!(Level::TRACE) {
            span.record("params", args.req().params());
        }
        self.oneshot_inner(args)
    }
}

// Some code is this file is reproduced under the terms of the MIT license. It
// originates from the `axum` crate. The original source code can be found at
// the following URL, and the original license is included below.
//
// https://github.com/tokio-rs/axum/
//
// The MIT License (MIT)
//
// Copyright (c) 2019 Axum Contributors
//
// Permission is hereby granted, free of charge, to any
// person obtaining a copy of this software and associated
// documentation files (the "Software"), to deal in the
// Software without restriction, including without
// limitation the rights to use, copy, modify, merge,
// publish, distribute, sublicense, and/or sell copies of
// the Software, and to permit persons to whom the Software
// is furnished to do so, subject to the following
// conditions:
//
// The above copyright notice and this permission notice
// shall be included in all copies or substantial portions
// of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
// ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
// TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
// PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
// SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
// OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
// IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.
