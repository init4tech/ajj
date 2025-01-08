use crate::routes::HandlerArgs;
use core::fmt;
use pin_project::pin_project;
use serde_json::value::RawValue;
use std::{
    convert::Infallible,
    future::Future,
    task::{Context, Poll},
};
use tower::util::{BoxCloneSyncService, Oneshot};

/// A future produced by
///
/// [`Route`]: crate::routes::Route
#[pin_project]
pub struct RouteFuture {
    /// The inner [`Route`] future.
    ///
    /// [`Route`]: crate::routes::Route
    #[pin]
    inner: Oneshot<BoxCloneSyncService<HandlerArgs, Box<RawValue>, Infallible>, HandlerArgs>,
    /// The span (if any).
    span: Option<tracing::Span>,
}

impl RouteFuture {
    /// Create a new route future.
    pub const fn new(
        inner: Oneshot<BoxCloneSyncService<HandlerArgs, Box<RawValue>, Infallible>, HandlerArgs>,
    ) -> Self {
        Self { inner, span: None }
    }

    /// Create a new method future with a span, this behaves as an
    /// [`Instrumented`], with the span entered when the future is polled.
    ///
    /// [`Instrumented`]: tracing::instrument::Instrumented
    pub const fn new_with_span(
        inner: Oneshot<BoxCloneSyncService<HandlerArgs, Box<RawValue>, Infallible>, HandlerArgs>,
        span: tracing::Span,
    ) -> Self {
        Self {
            inner,
            span: Some(span),
        }
    }

    /// Set the span for the future.
    pub fn with_span(self, span: tracing::Span) -> Self {
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
    type Output = Result<Box<RawValue>, Infallible>;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let _enter = this.span.as_ref().map(tracing::Span::enter);

        this.inner.poll(cx)
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
