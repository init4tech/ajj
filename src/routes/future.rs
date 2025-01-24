use crate::{routes::HandlerArgs, types::RequestError};
use core::fmt;
use pin_project::pin_project;
use serde_json::value::RawValue;
use std::{
    convert::Infallible,
    future::Future,
    panic,
    task::{ready, Context, Poll},
};
use tokio::task::JoinSet;
use tower::util::{BoxCloneSyncService, Oneshot};

use super::Response;

/// A future produced by [`Router::call_with_state`]. This should only be
/// instantiated by that method.
///
/// [`Route`]: crate::routes::Route
#[pin_project]
pub struct RouteFuture {
    /// The inner [`Route`] future.
    ///
    /// [`Route`]: crate::routes::Route
    #[pin]
    inner:
        Oneshot<BoxCloneSyncService<HandlerArgs, Option<Box<RawValue>>, Infallible>, HandlerArgs>,
    /// The span (if any).
    span: Option<tracing::Span>,
}

impl RouteFuture {
    /// Create a new route future.
    pub(crate) const fn new(
        inner: Oneshot<
            BoxCloneSyncService<HandlerArgs, Option<Box<RawValue>>, Infallible>,
            HandlerArgs,
        >,
    ) -> Self {
        Self { inner, span: None }
    }

    /// Set the span for the future.
    pub(crate) fn with_span(self, span: tracing::Span) -> Self {
        Self {
            span: Some(span),
            ..self
        }
    }
}

impl fmt::Debug for RouteFuture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RouteFuture").finish_non_exhaustive()
    }
}

impl Future for RouteFuture {
    type Output = Result<Option<Box<RawValue>>, Infallible>;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let _enter = this.span.as_ref().map(tracing::Span::enter);

        this.inner.poll(cx)
    }
}

#[pin_project(project = BatchFutureInnerProj)]
enum BatchFutureInner {
    Prepping(Vec<RouteFuture>),
    Running(#[pin] JoinSet<Result<Option<Box<RawValue>>, Infallible>>),
}

impl BatchFutureInner {
    fn len(&self) -> usize {
        match self {
            Self::Prepping(futs) => futs.len(),
            Self::Running(futs) => futs.len(),
        }
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn run(&mut self) {
        if let Self::Prepping(futs) = self {
            let js = futs.drain(..).collect::<JoinSet<_>>();
            *self = Self::Running(js);
        }
    }
}

impl fmt::Debug for BatchFutureInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("BatchFutureInner");

        match self {
            Self::Prepping(futs) => s.field("prepared", &futs.len()),
            Self::Running(futs) => s.field("running", &futs.len()),
        }
        .finish_non_exhaustive()
    }
}

/// A collection of [`RouteFuture`]s that are executed concurrently.
///
/// This is the type returned by [`Router::call_batch_with_state`], and should
/// only be instantiated by that method.
///
/// [`Router::call_batch_with_state`]: crate::Router::call_batch_with_state
#[pin_project]
pub struct BatchFuture {
    /// The futures, either in the prepping or running state.
    #[pin]
    futs: BatchFutureInner,
    /// The responses collected so far.
    resps: Vec<Box<RawValue>>,
    /// Whether the batch was a single request.
    single: bool,
}

impl fmt::Debug for BatchFuture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BatchFuture")
            .field("state", &self.futs)
            .field("responses", &self.resps.len())
            .finish()
    }
}

impl BatchFuture {
    /// Create a new batch future with a capacity.
    pub(crate) fn new_with_capacity(single: bool, capacity: usize) -> Self {
        Self {
            futs: BatchFutureInner::Prepping(Vec::with_capacity(capacity)),
            resps: Vec::with_capacity(capacity),
            single,
        }
    }

    /// Spawn a future into the batch.
    pub(crate) fn push(&mut self, fut: RouteFuture) {
        let BatchFutureInner::Prepping(ref mut futs) = self.futs else {
            panic!("pushing into a running batch future");
        };
        futs.push(fut);
    }

    /// Push a response into the batch.
    pub(crate) fn push_resp(&mut self, resp: Box<RawValue>) {
        self.resps.push(resp);
    }

    /// Push a parse error into the batch.
    pub(crate) fn push_parse_error(&mut self) {
        self.push_resp(Response::parse_error());
    }

    /// Push a parse result into the batch. Convenience function to simplify
    /// [`Router::call_batch_with_state`] logic.
    pub(crate) fn push_parse_result(&mut self, result: Result<RouteFuture, RequestError>) {
        match result {
            Ok(fut) => self.push(fut),
            Err(_) => self.push_parse_error(),
        }
    }
}

impl std::future::Future for BatchFuture {
    type Output = Option<Box<RawValue>>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        if matches!(self.futs, BatchFutureInner::Prepping(_)) {
            // SPEC: empty arrays are invalid
            if self.futs.is_empty() {
                return Poll::Ready(Some(Response::parse_error()));
            }
            self.futs.run();
        }

        let this = self.project();
        let BatchFutureInnerProj::Running(mut futs) = this.futs.project() else {
            unreachable!()
        };

        loop {
            match ready!(futs.poll_join_next(cx)) {
                Some(Ok(resp)) => {
                    // SPEC: notifications receive no response.
                    if let Some(resp) = unwrap_infallible!(resp) {
                        this.resps.push(resp);
                    }
                }

                // join set is drained, return the response(s)
                None => {
                    // SPEC: batches that contain only notifications receive no response.
                    if this.resps.is_empty() {
                        return Poll::Ready(None);
                    }

                    // SPEC: single requests return a single response
                    // Batch requests return an array of responses
                    let resp = if *this.single {
                        this.resps.pop().unwrap_or_else(|| Response::parse_error())
                    } else {
                        // otherwise, we have an array of responses
                        serde_json::value::to_raw_value(&this.resps)
                            .unwrap_or_else(|_| Response::serialization_failure(RawValue::NULL))
                    };

                    return Poll::Ready(Some(resp));
                }
                // panic in a future, propagate it
                Some(Err(err)) => {
                    tracing::error!(?err, "panic or cancel in batch future");
                    // propagate panics
                    if let Ok(reason) = err.try_into_panic() {
                        panic::resume_unwind(reason);
                    }
                }
            }
        }
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
