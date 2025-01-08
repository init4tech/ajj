use crate::routes::HandlerArgs;
use crate::{HandlerCtx, Route};
use alloy::rpc::json_rpc::{ResponsePayload, RpcObject};
use serde_json::value::RawValue;
use std::{convert::Infallible, future::Future, marker::PhantomData, pin::Pin, task};

/// A trait describing handlers for a JSON-RPC method.
///
/// Handlers map some input type `T` to a future that resolve to a
/// [`ResponsePayload`]. The handler may also require some state `S` to operate.
///
/// ## Note
///
/// The `S` type parameter is "missing" state. It represents the state that the
/// handler needs to operate. This state is passed to the handler when calling
/// [`Handler::call_with_state`]. A [`Handler`] can be converted into a
/// [`HandlerService`] by providing an owned state via the
/// [`Handler::with_state`] method.
///
/// ## Blanket Implementations
///
/// This trait is blanket implemented for
/// - fn(Parmas) -> Fut
/// - fn(Params, S) -> Fut
pub trait Handler<T, S>: Clone + Send + Sync + Sized + 'static {
    /// The future returned by the handler.
    type Future: Future<Output = ResponsePayload> + Send + 'static;

    /// Call the handler with the given request and state.
    fn call_with_state(self, ctx: HandlerCtx, params: Box<RawValue>, state: S) -> Self::Future;

    /// Create a new handler that wraps this handler and has some state.
    fn with_state(self, state: S) -> HandlerService<Self, T, S> {
        HandlerService::new(self, state)
    }

    /// Convert into a [`Route`] with the given state.
    fn into_route(self, state: S) -> Route
    where
        T: Send + 'static,
        S: Clone + Send + Sync + 'static,
    {
        Route::new(self.with_state(state))
    }
}

/// A [`Handler`] with some state `S` and params type `T`.
#[derive(Debug)]
pub struct HandlerService<H, T, S> {
    handler: H,
    state: S,
    _marker: std::marker::PhantomData<fn() -> T>,
}

impl<H, T, S> Clone for HandlerService<H, T, S>
where
    H: Clone,
    S: Clone,
{
    fn clone(&self) -> Self {
        Self {
            handler: self.handler.clone(),
            state: self.state.clone(),
            _marker: PhantomData,
        }
    }
}

impl<H, T, S> HandlerService<H, T, S> {
    /// Create a new handler service.
    pub const fn new(handler: H, state: S) -> Self {
        Self {
            handler,
            state,
            _marker: PhantomData,
        }
    }
}

impl<H, T, S> tower::Service<HandlerArgs> for HandlerService<H, T, S>
where
    Self: Clone,
    H: Handler<T, S>,
    T: Send + 'static,
    S: Clone + Send + Sync + 'static,
{
    type Response = ResponsePayload;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<ResponsePayload, Infallible>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut task::Context<'_>) -> task::Poll<Result<(), Self::Error>> {
        task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: HandlerArgs) -> Self::Future {
        let this = self.clone();
        Box::pin(async move {
            Ok(this
                .handler
                .call_with_state(request.0, request.1, this.state.clone())
                .await)
        })
    }
}

impl<F, Fut, Params, Payload, ErrData, S> Handler<(Params,), S> for F
where
    F: FnOnce(Params) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<Payload, ErrData>> + Send + 'static,
    Params: RpcObject,
    Payload: RpcObject,
    ErrData: RpcObject,
{
    type Future = Pin<Box<dyn Future<Output = ResponsePayload> + Send>>;

    fn call_with_state(self, _ctx: HandlerCtx, req: Box<RawValue>, _state: S) -> Self::Future {
        Box::pin(async move {
            let Ok(params) = serde_json::from_str(req.get()) else {
                return ResponsePayload::invalid_params();
            };

            self(params)
                .await
                .map(ResponsePayload::Success)
                .unwrap_or_else(ResponsePayload::internal_error_with_obj)
                .serialize_payload()
                .ok()
                .unwrap_or_else(ResponsePayload::internal_error)
        })
    }
}

impl<F, Fut, Params, Payload, ErrData, S> Handler<(HandlerCtx, Params), S> for F
where
    F: FnOnce(HandlerCtx, Params) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<Payload, ErrData>> + Send + 'static,
    Params: RpcObject,
    Payload: RpcObject,
    ErrData: RpcObject,
{
    type Future = Pin<Box<dyn Future<Output = ResponsePayload> + Send>>;

    fn call_with_state(self, ctx: HandlerCtx, req: Box<RawValue>, _state: S) -> Self::Future {
        Box::pin(async move {
            let Ok(params) = serde_json::from_str(req.get()) else {
                return ResponsePayload::invalid_params();
            };

            self(ctx, params)
                .await
                .map(ResponsePayload::Success)
                .unwrap_or_else(ResponsePayload::internal_error_with_obj)
                .serialize_payload()
                .ok()
                .unwrap_or_else(ResponsePayload::internal_error)
        })
    }
}

impl<F, Fut, Params, Payload, ErrData, S> Handler<(Params, S), S> for F
where
    F: FnOnce(Params, S) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<Payload, ErrData>> + Send + 'static,
    Params: RpcObject,
    Payload: RpcObject,
    ErrData: RpcObject,
    S: Send + Sync + 'static,
{
    type Future = Pin<Box<dyn Future<Output = ResponsePayload> + Send>>;

    fn call_with_state(self, _ctx: HandlerCtx, req: Box<RawValue>, state: S) -> Self::Future {
        Box::pin(async move {
            let Ok(params) = serde_json::from_str(req.get()) else {
                return ResponsePayload::invalid_params();
            };

            self(params, state)
                .await
                .map(ResponsePayload::Success)
                .unwrap_or_else(ResponsePayload::internal_error_with_obj)
                .serialize_payload()
                .ok()
                .unwrap_or_else(|| {
                    ResponsePayload::internal_error_message("Failed to serialize response".into())
                })
        })
    }
}

impl<F, Fut, Params, Payload, ErrData, S> Handler<(HandlerCtx, Params, S), S> for F
where
    F: FnOnce(HandlerCtx, Params, S) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<Payload, ErrData>> + Send + 'static,
    Params: RpcObject,
    Payload: RpcObject,
    ErrData: RpcObject,
    S: Send + Sync + 'static,
{
    type Future = Pin<Box<dyn Future<Output = ResponsePayload> + Send>>;

    fn call_with_state(self, ctx: HandlerCtx, req: Box<RawValue>, state: S) -> Self::Future {
        Box::pin(async move {
            let Ok(params) = serde_json::from_str(req.get()) else {
                return ResponsePayload::invalid_params();
            };

            self(ctx, params, state)
                .await
                .map(ResponsePayload::Success)
                .unwrap_or_else(ResponsePayload::internal_error_with_obj)
                .serialize_payload()
                .ok()
                .unwrap_or_else(|| {
                    ResponsePayload::internal_error_message("Failed to serialize response".into())
                })
        })
    }
}

// Some code is this file is reproduced under the terms of the MIT license. It
// originates from the `axum` crate. The original source code can be found at
// the following URL, and the original license is included below.
//
// https://github.com/tokio-rs/axum/
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
