use crate::{Handler, Route};
use serde_json::value::RawValue;
use tower::Service;

/// A boxed, erased type that can be converted into a [`Route`]. Similar to
/// axum's [`ErasedIntoRoute`]
///
/// Currently this is a placeholder to enable future convenience functions
///
/// [`ErasedIntoRoute`]: https://github.com/tokio-rs/axum/blob/f84105ae8b078109987b089c47febc3b544e6b80/axum/src/boxed.rs#L61-L68
pub(crate) trait ErasedIntoRoute<S>: Send + Sync {
    /// Take a reference to this type, clone it, box it, and type erase it.
    ///
    /// This allows it to be stored in a collection of `dyn
    /// ErasedIntoRoute<S>`.
    fn clone_box(&self) -> Box<dyn ErasedIntoRoute<S>>;

    /// Convert this type into a handler.
    fn into_route(self: Box<Self>, state: S) -> Route;

    /// Call this handler with the given state.
    #[allow(dead_code)]
    fn call_with_state(
        self: Box<Self>,
        params: Box<RawValue>,
        state: S,
    ) -> <Route as Service<Box<RawValue>>>::Future;
}

/// A boxed, erased type that can be converted into a [`Route`]. It is a
/// wrapper around a dyn [`ErasedIntoRoute`].
///
/// Similar to axum's [`BoxedIntoRoute`]
///
/// Currently this is a placeholder to enable future convenience functions.
///
/// [`BoxedIntoRoute`]: https://github.com/tokio-rs/axum/blob/18a99da0b0baf9eeef326b34525826ae0b5a1370/axum/src/boxed.rs#L12
pub(crate) struct BoxedIntoRoute<S>(pub(crate) Box<dyn ErasedIntoRoute<S>>);

#[allow(dead_code)]
impl<S> BoxedIntoRoute<S> {
    /// Convert this into a [`Route`] with the given state.
    pub fn into_route(self, state: S) -> Route {
        self.0.into_route(state)
    }
}

impl<S> Clone for BoxedIntoRoute<S> {
    fn clone(&self) -> Self {
        Self(self.0.clone_box())
    }
}

#[allow(dead_code)]
impl<S> BoxedIntoRoute<S> {
    pub(crate) fn from_handler<H, T>(handler: H) -> Self
    where
        H: Handler<T, S>,
        T: Send + 'static,
        S: Clone + Send + Sync + 'static,
    {
        Self(Box::new(MakeErasedHandler::from_handler(handler)))
    }
}

/// Adapter to convert [`Handler`]s into [`ErasedIntoRoute`]s.
pub(crate) struct MakeErasedHandler<H, S> {
    pub(crate) handler: H,
    pub(crate) into_route: fn(H, S) -> Route,
}

impl<H, S> Clone for MakeErasedHandler<H, S>
where
    H: Clone,
{
    fn clone(&self) -> Self {
        Self {
            handler: self.handler.clone(),
            into_route: self.into_route,
        }
    }
}

impl<H, S> MakeErasedHandler<H, S> {
    /// Create a new [`MakeErasedHandler`] with the given handler and conversion
    /// function.
    pub fn from_handler<T>(handler: H) -> Self
    where
        H: Handler<T, S>,
        T: Send + 'static,
        S: Clone + Send + Sync + 'static,
    {
        MakeErasedHandler {
            handler,
            into_route: |handler, state| Handler::into_route(handler, state),
        }
    }
}

impl<H, S> ErasedIntoRoute<S> for MakeErasedHandler<H, S>
where
    H: Clone + Send + Sync + 'static,
    S: 'static,
{
    fn clone_box(&self) -> Box<dyn ErasedIntoRoute<S>> {
        Box::new(self.clone())
    }

    fn into_route(self: Box<Self>, state: S) -> Route {
        (self.into_route)(self.handler, state)
    }

    fn call_with_state(
        self: Box<Self>,
        params: Box<RawValue>,
        state: S,
    ) -> <Route as Service<Box<RawValue>>>::Future {
        self.into_route(state).call(params)
    }
}

// Some code is this file is reproduced under the terms of the MIT license. It
// originates from the `axum` crate. The original source code can be found at
// the following URL, and the original license is included below.
//
// https://github.com/tokio-rs/axum/blob/f84105ae8b078109987b089c47febc3b544e6b80/axum/src/routing/mod.rs#L119
//
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
