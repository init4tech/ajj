use crate::{routes::RouteFuture, BoxedIntoRoute, HandlerArgs, Route};

/// A method, which may be ready to handle requests or may need to be
/// initialized with some state.
///
/// Analagous to axum's `MethodEndpoint`
pub(crate) enum Method<S> {
    /// A method that needs to be initialized with some state.
    Needs(BoxedIntoRoute<S>),
    /// A method that is ready to handle requests.
    Ready(Route),
}

impl<S> Clone for Method<S> {
    fn clone(&self) -> Self {
        match self {
            Self::Needs(handler) => Self::Needs(handler.clone()),
            Self::Ready(route) => Self::Ready(route.clone()),
        }
    }
}

impl<S> Method<S> {
    /// Call the method with the given state and request.
    pub(crate) fn call_with_state(&self, args: HandlerArgs, state: S) -> RouteFuture {
        match self {
            Self::Ready(route) => route.clone().oneshot_inner_owned(args),
            Self::Needs(handler) => handler
                .clone()
                .0
                .into_route(state)
                .oneshot_inner_owned(args),
        }
    }
}

impl<S> Method<S>
where
    S: Clone,
{
    /// Add state to a method, converting
    pub(crate) fn with_state<S2>(self, state: &S) -> Method<S2> {
        match self {
            Self::Ready(route) => Method::Ready(route),
            Self::Needs(handler) => Method::Ready(handler.0.into_route(state.clone())),
        }
    }
}

// Some code is this file is reproduced under the terms of the MIT license. It
// originates from the `axum` crate. The original source code can be found at
// the following URL, and the original license is included below.
//
// https://github.com/tokio-rs/axum/blob/f84105ae8b078109987b089c47febc3b544e6b80/axum/src/routing/mod.rs#L119
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
