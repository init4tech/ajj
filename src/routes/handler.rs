use crate::{
    routes::HandlerArgs, types::Response, HandlerCtx, ResponsePayload, Route, RpcRecv, RpcSend,
};
use serde_json::value::RawValue;
use std::{convert::Infallible, future::Future, marker::PhantomData, pin::Pin, task};

macro_rules! convert_result {
    ($res:expr) => {{
        match $res {
            Ok(val) => ResponsePayload::Success(val),
            Err(err) => ResponsePayload::internal_error_with_obj(err),
        }
    }};
}

/// A trait describing handlers for JSON-RPC methods.
///
/// Handlers map some input type `T` to a future that resolve to a
/// [`ResponsePayload`]. The handler may also require some state `S` to operate.
///
/// ### Returning error messages
///
/// Note that when a [`Handler`] returns a `Result`, the error type will be in
/// the [`ResponsePayload`]'s `data` field, and the message and code will
/// indicate an internal server error. To return a response with an error code
/// and custom `message` field, use a handler that returns an instance of the
/// [`ResponsePayload`] enum, and instantiate that error payload manually.
///
/// ```
/// # use ajj::ResponsePayload;
/// let handler_a = || async { Err::<(), _>("appears in \"data\"") };
/// let handler_b = || async {
///     ResponsePayload::<(), ()>::internal_error_message("appears in \"message\"".into())
/// };
/// ```
///
/// ### Handler return type inference
///
/// Handlers that always suceed or always fail may have trouble with type
/// inference, as they contain an unknown type parameter, which could be
/// anything. Here's an example of code with failed type inference:
///
/// ```compile_fail
/// # use ajj::ResponsePayload;
/// // cannot infer type of the type parameter `T` declared on the enum `Result`
/// let cant_infer_ok = || async { Err(1) };
///
/// // cannot infer type of the type parameter `E` declared on the enum `Result`
/// let cant_infer_err = || async { Ok(2) };
///
/// // cannot infer type of the type parameter `ErrData` declared on the enum `ResponsePayload`
/// let cant_infer_failure = || async { ResponsePayload::Success(3) };
///
/// // cannot infer type of the type parameter `ErrData` declared on the enum `ResponsePayload`
/// let cant_infer_success = || async { ResponsePayload::internal_error_with_obj(4) };
/// ```
///
/// If you encounter these sorts of inference errors, you can add turbofish to
/// your handlers' return values like so:
///
/// ```
/// # use ajj::ResponsePayload;
/// // specify the Err on your Ok
/// let handler_a = || async { Ok::<_, ()>(1) };
///
/// // specify the Ok on your Err
/// let handler_b = || async { Err::<(), _>(2) };
///
/// // specify the ErrData on your Success
/// let handler_c = || async { ResponsePayload::Success::<_, ()>(3) };
///
/// // specify the Payload on your Failure
/// let handler_d = || async { ResponsePayload::<(), _>::internal_error_with_obj(4) };
/// ```
///
/// ## Notifications
///
/// When running on pubsub, handlers can send notifications to the client. This
/// is done by calling [`HandlerCtx::notify`]. Notifications are sent as JSON
/// objects, and are queued for sending to the client. If the client is not
/// reading from the connection, the notification will be queued in a buffer.
/// If the buffer is full, the handler will be backpressured until the buffer
/// has room.
///
/// We recommend that handlers `await` on the result of [`HandlerCtx::notify`],
/// to ensure that they are backpressured when the notification buffer is full.
/// If many tasks are attempting to notify the same client, the buffer may fill
/// up, and backpressure the `RouteTask` from reading requests from the
/// connection. This can lead to delays in request processing.
///
#[cfg_attr(
    feature = "pubsub",
    doc = "see the [`Listener`] documetnation for more information."
)]
///
/// ## Note on `S`
///
/// The `S` type parameter is "missing" state. It represents the state that the
/// handler needs to operate. This state is passed to the handler when calling
/// [`Handler::call_with_state`].
///
/// ## Blanket Implementations
///
/// This trait is blanket implemented for the following function and closure
/// types, where `Fut` is a [`Future`] returning either [`ResponsePayload`] or
/// [`Result`]:
///
/// - `async fn()`
/// - `async fn(Params) -> Fut`
/// - `async fn(HandlerCtx, Params) -> Fut`
/// - `async fn(Params, S) -> Fut`
/// - `async fn(HandlerCtx, Params) -> Fut`
/// - `async fn(HandlerCtx, Params, S) -> Fut`
///
/// ### Implementer's note:
///
/// The generics on the implementation of the `Handler` trait are actually very
/// straightforward, even though they look intimidating.
///
/// The `T` type parameter is a **marker** and never needs be constructed.
/// It exists to differentiate the output impls, so that the trait can be
/// blanket implemented for many different function types. If it were simply
/// `Handler<S>`, there could only be 1 blanket impl.
///
/// However the `T` type must constrain the relevant input types. When
/// implementing `Handler<T, S>` for your own type, it is recommended to do the
/// following:
///
/// ```
/// use ajj::{Handler, HandlerArgs, ResponsePayload, RawValue};
/// use std::{pin::Pin, future::Future};
///
/// /// A marker type to differentiate this handler from other blanket impls.
/// pub struct MyMarker {
///   _sealed: (),
/// }
///
/// #[derive(Clone)]
/// pub struct MyHandler;
///
/// // the `T` type parameter should be a tuple, containing your marker type,
/// // and the components that your handler actually uses. This ensures that
/// // the implementations are always unambiguous.
/// //
/// // e.g.
/// // - (MyMarker, )
/// // - (MyMarker, HandlerArgs)
/// // - (MyMarker, HandlerArgs, Params)
/// // etc.
///
/// impl<S> Handler<(MyMarker, ), S> for MyHandler {
///   type Future = Pin<Box<dyn Future<Output = Option<Box<RawValue>>> + Send>>;
///
///   fn call_with_state(self, _args: HandlerArgs, _state: S) -> Self::Future {
///     todo!("use nothing but your struct")
///   }
/// }
///
/// impl<S> Handler<(MyMarker, HandlerArgs), S> for MyHandler {
///   type Future = Pin<Box<dyn Future<Output = Option<Box<RawValue>>> + Send>>;
///
///   fn call_with_state(self, args: HandlerArgs, _state: S) -> Self::Future {
///     todo!("use the args")
///   }
/// }
/// ```
#[cfg_attr(feature = "pubsub", doc = " [`Listener`]: crate::pubsub::Listener")]
pub trait Handler<T, S>: Clone + Send + Sync + Sized + 'static {
    /// The future returned by the handler.
    type Future: Future<Output = Option<Box<RawValue>>> + Send + 'static;

    /// Call the handler with the given request and state.
    fn call_with_state(self, args: HandlerArgs, state: S) -> Self::Future;
}

/// Extension trait for [`Handler`]s.
pub(crate) trait HandlerInternal<T, S>: Handler<T, S> {
    /// Create a new handler that wraps this handler and has some state.
    fn with_state(self, state: S) -> HandlerService<Self, T, S> {
        HandlerService::new(self, state)
    }

    /// Convert the handler into a [`Route`], ready for internal registration
    /// on a [`Router`].
    ///
    /// [`Router`]: crate::Router
    #[allow(private_interfaces)]
    fn into_route(self, state: S) -> Route
    where
        T: Send + 'static,
        S: Clone + Send + Sync + 'static,
    {
        Route::new(self.with_state(state))
    }
}

impl<T, S, H> HandlerInternal<T, S> for H where H: Handler<T, S> {}

/// A [`Handler`] with some state `S` and params type `T`.
#[derive(Debug)]
pub(crate) struct HandlerService<H, T, S> {
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
    pub(crate) const fn new(handler: H, state: S) -> Self {
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
    type Response = Option<Box<RawValue>>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Option<Box<RawValue>>, Infallible>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut task::Context<'_>) -> task::Poll<Result<(), Self::Error>> {
        task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, args: HandlerArgs) -> Self::Future {
        let this = self.clone();
        Box::pin(async move { Ok(this.handler.call_with_state(args, this.state.clone()).await) })
    }
}

/// A marker type for handlers that return a [`Result`].
///
/// This type should never be constructed, and importing it is almost certainly
/// a mistake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputResult {
    _sealed: (),
}

/// A marker type for handlers that return a [`ResponsePayload`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputResponsePayload {
    _sealed: (),
}

impl<F, Fut, Payload, ErrData, S> Handler<(OutputResponsePayload,), S> for F
where
    F: FnOnce() -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = ResponsePayload<Payload, ErrData>> + Send + 'static,
    Payload: RpcSend,
    ErrData: RpcSend,
{
    type Future = Pin<Box<dyn Future<Output = Option<Box<RawValue>>> + Send>>;

    fn call_with_state(self, args: HandlerArgs, _state: S) -> Self::Future {
        let id = args.req.id_owned();

        Box::pin(async move {
            let payload = self().await;

            Response::maybe(id.as_deref(), &payload)
        })
    }
}

impl<F, Fut, Payload, ErrData, S> Handler<(OutputResponsePayload, HandlerCtx), S> for F
where
    F: FnOnce(HandlerCtx) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = ResponsePayload<Payload, ErrData>> + Send + 'static,
    Payload: RpcSend,
    ErrData: RpcSend,
{
    type Future = Pin<Box<dyn Future<Output = Option<Box<RawValue>>> + Send>>;

    fn call_with_state(self, args: HandlerArgs, _state: S) -> Self::Future {
        let id = args.req.id_owned();
        let ctx = args.ctx;

        Box::pin(async move {
            let payload = self(ctx).await;
            Response::maybe(id.as_deref(), &payload)
        })
    }
}

impl<F, Fut, Params, Payload, ErrData, S> Handler<(OutputResponsePayload, Params), S> for F
where
    F: FnOnce(Params) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = ResponsePayload<Payload, ErrData>> + Send + 'static,
    Params: RpcRecv,
    Payload: RpcSend,
    ErrData: RpcSend,
{
    type Future = Pin<Box<dyn Future<Output = Option<Box<RawValue>>> + Send>>;

    fn call_with_state(self, args: HandlerArgs, _state: S) -> Self::Future {
        let HandlerArgs { req, .. } = args;
        Box::pin(async move {
            let id = req.id_owned();
            let Ok(params) = req.deser_params() else {
                return Response::maybe_invalid_params(id.as_deref());
            };

            let payload = self(params).await;
            Response::maybe(id.as_deref(), &payload)
        })
    }
}

impl<F, Fut, Params, Payload, ErrData, S> Handler<(OutputResponsePayload, HandlerCtx, Params), S>
    for F
where
    F: FnOnce(HandlerCtx, Params) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = ResponsePayload<Payload, ErrData>> + Send + 'static,
    Params: RpcRecv,
    Payload: RpcSend,
    ErrData: RpcSend,
{
    type Future = Pin<Box<dyn Future<Output = Option<Box<RawValue>>> + Send>>;

    fn call_with_state(self, args: HandlerArgs, _state: S) -> Self::Future {
        Box::pin(async move {
            let HandlerArgs { ctx, req } = args;

            let id = req.id_owned();
            let Ok(params) = req.deser_params() else {
                return Response::maybe_invalid_params(id.as_deref());
            };

            drop(req); // deallocate explicitly. No funny business.

            let payload = self(ctx, params).await;
            Response::maybe(id.as_deref(), &payload)
        })
    }
}

impl<F, Fut, Params, Payload, ErrData, S> Handler<(OutputResponsePayload, Params, S), S> for F
where
    F: FnOnce(Params, S) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = ResponsePayload<Payload, ErrData>> + Send + 'static,
    Params: RpcRecv,
    Payload: RpcSend,
    ErrData: RpcSend,
    S: Send + Sync + 'static,
{
    type Future = Pin<Box<dyn Future<Output = Option<Box<RawValue>>> + Send>>;

    fn call_with_state(self, args: HandlerArgs, state: S) -> Self::Future {
        Box::pin(async move {
            let HandlerArgs { req, .. } = args;

            let id = req.id_owned();
            let Ok(params) = req.deser_params() else {
                return Response::maybe_invalid_params(id.as_deref());
            };
            drop(req); // deallocate explicitly. No funny business.

            let payload = self(params, state).await;

            Response::maybe(id.as_deref(), &payload)
        })
    }
}

impl<F, Fut, Payload, ErrData, S> Handler<(OutputResponsePayload, S, HandlerCtx), S> for F
where
    F: FnOnce(HandlerCtx, S) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = ResponsePayload<Payload, ErrData>> + Send + 'static,
    Payload: RpcSend,
    ErrData: RpcSend,
    S: Send + Sync + 'static,
{
    type Future = Pin<Box<dyn Future<Output = Option<Box<RawValue>>> + Send>>;

    fn call_with_state(self, args: HandlerArgs, state: S) -> Self::Future {
        Box::pin(async move {
            let HandlerArgs { ctx, req } = args;

            let id = req.id_owned();

            drop(req); // deallocate explicitly. No funny business.

            let payload = self(ctx, state).await;

            Response::maybe(id.as_deref(), &payload)
        })
    }
}

impl<F, Fut, Params, Payload, ErrData, S> Handler<(OutputResponsePayload, HandlerCtx, Params, S), S>
    for F
where
    F: FnOnce(HandlerCtx, Params, S) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = ResponsePayload<Payload, ErrData>> + Send + 'static,
    Params: RpcRecv,
    Payload: RpcSend,
    ErrData: RpcSend,
    S: Send + Sync + 'static,
{
    type Future = Pin<Box<dyn Future<Output = Option<Box<RawValue>>> + Send>>;

    fn call_with_state(self, args: HandlerArgs, state: S) -> Self::Future {
        Box::pin(async move {
            let HandlerArgs { ctx, req } = args;

            let id = req.id_owned();
            let Ok(params) = req.deser_params() else {
                return Response::maybe_invalid_params(id.as_deref());
            };

            drop(req); // deallocate explicitly. No funny business.

            let payload = self(ctx, params, state).await;
            Response::maybe(id.as_deref(), &payload)
        })
    }
}

impl<F, Fut, Payload, ErrData, S> Handler<(OutputResult,), S> for F
where
    F: FnOnce() -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<Payload, ErrData>> + Send + 'static,
    Payload: RpcSend,
    ErrData: RpcSend,
{
    type Future = Pin<Box<dyn Future<Output = Option<Box<RawValue>>> + Send>>;

    fn call_with_state(self, args: HandlerArgs, _state: S) -> Self::Future {
        let id = args.req.id_owned();
        drop(args);
        Box::pin(async move {
            let payload = self().await;
            Response::maybe(id.as_deref(), &convert_result!(payload))
        })
    }
}

impl<F, Fut, Payload, ErrData, S> Handler<(OutputResult, HandlerCtx), S> for F
where
    F: FnOnce(HandlerCtx) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<Payload, ErrData>> + Send + 'static,
    Payload: RpcSend,
    ErrData: RpcSend,
{
    type Future = Pin<Box<dyn Future<Output = Option<Box<RawValue>>> + Send>>;

    fn call_with_state(self, args: HandlerArgs, _state: S) -> Self::Future {
        let HandlerArgs { ctx, req } = args;

        let id = req.id_owned();

        drop(req);

        Box::pin(async move {
            let payload = convert_result!(self(ctx).await);
            Response::maybe(id.as_deref(), &payload)
        })
    }
}

impl<F, Fut, Params, Payload, ErrData, S> Handler<(OutputResult, Params), S> for F
where
    F: FnOnce(Params) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<Payload, ErrData>> + Send + 'static,
    Params: RpcRecv,
    Payload: RpcSend,
    ErrData: RpcSend,
{
    type Future = Pin<Box<dyn Future<Output = Option<Box<RawValue>>> + Send>>;

    fn call_with_state(self, args: HandlerArgs, _state: S) -> Self::Future {
        Box::pin(async move {
            let HandlerArgs { req, .. } = args;

            let id = req.id_owned();
            let Ok(params) = req.deser_params() else {
                return Response::maybe_invalid_params(id.as_deref());
            };

            drop(req); // deallocate explicitly. No funny business.

            let payload = convert_result!(self(params).await);

            Response::maybe(id.as_deref(), &payload)
        })
    }
}

impl<F, Fut, Params, Payload, ErrData, S> Handler<(OutputResult, HandlerCtx, Params), S> for F
where
    F: FnOnce(HandlerCtx, Params) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<Payload, ErrData>> + Send + 'static,
    Params: RpcRecv,
    Payload: RpcSend,
    ErrData: RpcSend,
{
    type Future = Pin<Box<dyn Future<Output = Option<Box<RawValue>>> + Send>>;

    fn call_with_state(self, args: HandlerArgs, _state: S) -> Self::Future {
        Box::pin(async move {
            let HandlerArgs { ctx, req } = args;

            let id = req.id_owned();
            let Ok(params) = req.deser_params() else {
                return Response::maybe_invalid_params(id.as_deref());
            };

            drop(req); // deallocate explicitly. No funny business.

            let payload = convert_result!(self(ctx, params).await);

            Response::maybe(id.as_deref(), &payload)
        })
    }
}

impl<F, Fut, Params, Payload, ErrData, S> Handler<(OutputResult, Params, S), S> for F
where
    F: FnOnce(Params, S) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<Payload, ErrData>> + Send + 'static,
    Params: RpcRecv,
    Payload: RpcSend,
    ErrData: RpcSend,
    S: Send + Sync + 'static,
{
    type Future = Pin<Box<dyn Future<Output = Option<Box<RawValue>>> + Send>>;

    fn call_with_state(self, args: HandlerArgs, state: S) -> Self::Future {
        Box::pin(async move {
            let HandlerArgs { req, .. } = args;

            let id = req.id_owned();
            let Ok(params) = req.deser_params() else {
                return Response::maybe_invalid_params(id.as_deref());
            };

            drop(req); // deallocate explicitly. No funny business.

            let payload = convert_result!(self(params, state).await);

            Response::maybe(id.as_deref(), &payload)
        })
    }
}

impl<F, Fut, Payload, ErrData, S> Handler<(OutputResult, S, HandlerCtx), S> for F
where
    F: FnOnce(HandlerCtx, S) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<Payload, ErrData>> + Send + 'static,
    Payload: RpcSend,
    ErrData: RpcSend,
    S: Send + Sync + 'static,
{
    type Future = Pin<Box<dyn Future<Output = Option<Box<RawValue>>> + Send>>;

    fn call_with_state(self, args: HandlerArgs, state: S) -> Self::Future {
        let HandlerArgs { ctx, req } = args;

        let id = req.id_owned();

        drop(req);

        Box::pin(async move {
            let payload = convert_result!(self(ctx, state).await);

            Response::maybe(id.as_deref(), &payload)
        })
    }
}

impl<F, Fut, Params, Payload, ErrData, S> Handler<(OutputResult, HandlerCtx, Params, S), S> for F
where
    F: FnOnce(HandlerCtx, Params, S) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<Payload, ErrData>> + Send + 'static,
    Params: RpcRecv,
    Payload: RpcSend,
    ErrData: RpcSend,
    S: Send + Sync + 'static,
{
    type Future = Pin<Box<dyn Future<Output = Option<Box<RawValue>>> + Send>>;

    fn call_with_state(self, args: HandlerArgs, state: S) -> Self::Future {
        Box::pin(async move {
            let HandlerArgs { ctx, req } = args;

            let id = req.id_owned();
            let Ok(params) = req.deser_params() else {
                return Response::maybe_invalid_params(id.as_deref());
            };

            drop(req); // deallocate explicitly. No funny business.

            let payload = convert_result!(self(ctx, params, state).await);

            Response::maybe(id.as_deref(), &payload)
        })
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
