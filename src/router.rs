//! JSON-RPC router.

use crate::{
    routes::{BatchFuture, MakeErasedHandler, RouteFuture},
    types::InboundData,
    BoxedIntoRoute, ErasedIntoRoute, Handler, HandlerArgs, HandlerCtx, Method, MethodId,
    RegistrationError, Route,
};
use core::fmt;
use serde_json::value::RawValue;
use std::{borrow::Cow, collections::BTreeMap, convert::Infallible, sync::Arc, task::Poll};
use tower::Service;
use tracing::debug_span;

/// A JSON-RPC router. This is the top-level type for handling JSON-RPC
/// requests. It is heavily inspired by the [`axum::Router`] type.
///
/// A router manages a collection of "methods" that can be called by clients.
/// Each method is associated with a handler that processes the request and
/// returns a response. The router is responsible for routing requests to the
/// appropriate handler.
///
/// Methods can be added to the router using the [`Router::route`] family of
/// methods:
///
/// - [`Router::route`]: Add a method with a [`Handler`]. The [`Handler`] will
///   be invoked with the request parameters and provided state.
///
/// ## Basic Example
///
/// Routers are constructed via builder-style methods. The following example
/// demonstrates how to create a router with two methods, `double` and `add`.
/// The `double` method doubles the provided number, and the `add` method adds
/// the provided number to some state, and returns the sum.
///
/// ```
/// use ajj::Router;
///
/// # fn main() {
/// // Provide methods called "double" and "add" to the router.
/// let router = Router::<u64>::new()
///    // This route requires state to be provided.
///   .route("add", |params: u64, state: u64| async move {
///     Ok::<_, ()>(params + state)
///   })
///   .with_state::<()>(3u64)
///   // when double is called, the provided number is doubled.
///   // see the Handler docs for more information on the hint type
///   .route("double", |params: u64| async move {
///     Ok::<_, ()>(params * 2)
///   });
/// # }
/// ```
///
/// ## Note
///
/// The state `S` is "missing" state. It is state that must be added to the
/// router (and therefore to the methods) before it can be used. To add state,
/// use the [`Router::with_state`] method. See that method for more
/// information.
///
/// Analagous to [`axum::Router`].
///
/// [`axum::Router`]: https://docs.rs/axum/latest/axum/struct.Router.html
#[must_use = "Routers do nothing unless served."]
pub struct Router<S> {
    inner: Arc<RouterInner<S>>,
}

impl<S> Clone for Router<S> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<S> Default for Router<S>
where
    S: Send + Sync + Clone + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<S> Router<S>
where
    S: Send + Sync + Clone + 'static,
{
    /// Create a new, empty router.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RouterInner::new()),
        }
    }

    /// If this router is the only reference to its inner state, return the
    /// inner state. Otherwise, clone the inner state and return the clone.
    fn into_inner(self) -> RouterInner<S> {
        match Arc::try_unwrap(self.inner) {
            Ok(inner) => inner,
            Err(arc) => RouterInner {
                routes: arc.routes.clone(),
                last_id: arc.last_id,
                fallback: arc.fallback.clone(),
                name_to_id: arc.name_to_id.clone(),
                id_to_name: arc.id_to_name.clone(),
            },
        }
    }

    /// Add state to the router, readying methods that require that state.
    ///
    /// ## Note
    ///
    /// The type parameter `S2` is NOT the state you are adding to the
    /// router. It is additional state that must be added AFTER the state `S`.
    pub fn with_state<S2>(self, state: S) -> Router<S2> {
        map_inner!(self, inner => inner.with_state(&state))
    }

    /// Add a fallback [`Route`]. This route will be called when no method
    /// names match the request.
    ///
    /// ## Note
    ///
    /// If unset, a default fallback route will be used that returns the error
    /// generated by [`ResponsePayload::method_not_found`].
    ///
    /// [`ResponsePayload::method_not_found`]: crate::ResponsePayload::method_not_found
    pub(crate) fn fallback_stateless(self, route: Route) -> Self {
        tap_inner!(self, mut this => {
            this.fallback = Method::Ready(route);
        })
    }

    /// Add a fallback [`ErasedIntoRoute`] handler. The handler will be cloned
    /// and boxed. This handler will be called when no method names match the
    /// request.
    ///
    /// ## Notes
    ///
    /// The `S` type parameter is "missing" state. It is state that must be
    /// added to the router (and therefore to the methods) before it can be
    /// used. To add state, use the [`Router::with_state`] method. See that
    /// method for more information.
    ///
    /// If unset, a default fallback route will be used that returns the error
    /// generated by [`ResponsePayload::method_not_found`].
    ///
    /// [`ResponsePayload::method_not_found`]: crate::ResponsePayload::method_not_found
    fn fallback_erased<E>(self, handler: E) -> Self
    where
        E: ErasedIntoRoute<S>,
    {
        tap_inner!(self, mut this => {
            this.fallback = Method::Needs(BoxedIntoRoute(handler.clone_box()));
        })
    }

    /// Add a fallback [`Handler`] to the router. This handler will be called
    /// when no method names match the request.
    ///
    /// ## Notes
    ///
    /// The `S` type parameter is "missing" state. It is state that must be
    /// added to the router (and therefore to the methods) before it can be
    /// used. To add state, use the [`Router::with_state`] method. See that
    /// method for more information.
    ///
    /// If unset, a default fallback route will be used that returns the error
    /// generated by [`ResponsePayload::method_not_found`].
    ///
    /// [`ResponsePayload::method_not_found`]: crate::ResponsePayload::method_not_found
    pub fn fallback<H, T>(self, handler: H) -> Self
    where
        H: Handler<T, S>,
        T: Send + 'static,
        S: Clone + Send + Sync + 'static,
    {
        self.fallback_erased(MakeErasedHandler::from_handler(handler))
    }

    /// Add a fallback [`Service`] handler. This handler will be called when no
    /// method names match the request.
    ///
    /// ## Note
    ///
    /// If unset, a default fallback route will be used that returns the error
    /// generated by [`ResponsePayload::method_not_found`].
    ///
    /// [`ResponsePayload::method_not_found`]: crate::ResponsePayload::method_not_found
    pub fn fallback_service<T>(self, service: T) -> Self
    where
        T: Service<
                HandlerArgs,
                Response = Option<Box<RawValue>>,
                Error = Infallible,
                Future: Send + 'static,
            > + Clone
            + Send
            + Sync
            + 'static,
    {
        self.fallback_stateless(Route::new(service))
    }

    /// Add a method with a [`Handler`] to the router.
    ///
    /// ## Note
    ///
    /// The `S` type parameter is "missing" state. It is state that must be
    /// added to the router (and therefore to the methods) before it can be
    /// used. To add state, use the [`Router::with_state`] method. See that
    /// method for more information.
    ///
    /// # Panics
    ///
    /// Panics if the method name already exists in the router.
    pub fn route<H, T>(self, method: impl Into<Cow<'static, str>>, handler: H) -> Self
    where
        H: Handler<T, S>,
        T: Send + 'static,
        S: Clone + Send + Sync + 'static,
    {
        tap_inner!(self, mut this => {
            this = this.route(method, handler);
        })
    }

    /// Remove a method from the router.
    ///
    /// If the route does not exist, this method is a no-op.
    pub fn remove_route(self, method: impl Into<Cow<'static, str>>) -> Self {
        tap_inner!(self, mut this => {
            this = this.remove_route(method.into());
        })
    }

    /// Nest a router under a prefix. This is useful for grouping related
    /// methods together, or for namespacing logical groups of methods.
    ///
    /// The prefix is trimmed of any trailing underscores, and a single
    /// underscore is added between the prefix and the method name.
    ///
    /// I.e. the following are equivalent and will all produce the `foo_`
    /// prefix for the methods in `other`:
    /// - `router.nest("foo", other)`
    /// - `router.nest("foo_", other)`
    /// - `router.nest("foo__", other)`
    ///
    /// # Panics
    ///
    /// Panics if collision occurs between the prefixed methods from `other` and
    /// the methods in `self`. E.g. if the prefix is `foo`, `self` has a
    /// method `foo_bar`, and `other` has a method `bar`.
    pub fn nest(self, prefix: impl Into<Cow<'static, str>>, other: Self) -> Self {
        let prefix = prefix.into();

        let mut this = self.into_inner();
        let prefix = Cow::Borrowed(prefix.trim_end_matches('_'));

        let RouterInner {
            routes, id_to_name, ..
        } = other.into_inner();

        for (id, handler) in routes.into_iter() {
            let existing_name = id_to_name
                .get(&id)
                .expect("nested router has missing name for existing method");
            let method = format!("{}_{}", prefix, existing_name);
            panic_on_err!(this.enroll_method(method.into(), handler));
        }

        Self {
            inner: Arc::new(this),
        }
    }

    /// Merge two routers together. This enrolls all methods from `other` into
    /// `self`.
    ///
    /// # Panics
    ///
    /// Panics if a method name collision occurs between the two routers. I.e.
    /// if any method from `other` has the same name as a method in `self`.
    pub fn merge(self, other: Self) -> Self {
        let mut this = self.into_inner();
        let RouterInner {
            routes,
            mut id_to_name,
            ..
        } = other.into_inner();

        for (id, handler) in routes.into_iter() {
            let existing_name = id_to_name
                .remove(&id)
                .expect("nested router has missing name for existing method");
            panic_on_err!(this.enroll_method(existing_name, handler));
        }

        Self {
            inner: Arc::new(this),
        }
    }

    /// Call a method on the router, by providing the necessary state.
    ///
    /// This is a convenience method, primarily for testing. Use in production
    /// code is discouraged. Routers should not be left in incomplete states.
    pub fn call_with_state(&self, args: HandlerArgs, state: S) -> RouteFuture {
        let id = args.req().id_owned();
        let method = args.req().method();
        let span = debug_span!(parent: None, "Router::call_with_state", %method, ?id);

        self.inner.call_with_state(args, state).with_span(span)
    }

    /// Call a method on the router, without providing state.
    pub fn call_batch_with_state(
        &self,
        ctx: HandlerCtx,
        inbound: InboundData,
        state: S,
    ) -> BatchFuture {
        let mut fut = BatchFuture::new_with_capacity(inbound.single(), inbound.len());
        // According to spec, non-parsable requests should still receive a
        // response.
        let span = debug_span!(parent: None, "BatchFuture::poll", reqs = inbound.len(), futs = tracing::field::Empty);

        for (batch_idx, req) in inbound.iter().enumerate() {
            let req = req.map(|req| {
                let span = debug_span!(parent: &span, "RouteFuture::poll", batch_idx, method = req.method(), id = ?req.id());
                let args = HandlerArgs::new(ctx.clone(), req);
                self.inner
                    .call_with_state(args, state.clone())
                    .with_span(span)
            });
            fut.push_parse_result(req);
        }
        span.record("futs", fut.len());
        fut.with_span(span)
    }

    /// Nest this router into a new Axum router, with the specified path and the currently-running
    ///
    /// Users needing specific control over the runtime should use
    /// [`Router::into_axum_with_handle`] instead.
    #[cfg(feature = "axum")]
    pub fn into_axum(self, path: &str) -> axum::Router<S> {
        axum::Router::new().route(path, axum::routing::post(crate::axum::IntoAxum::from(self)))
    }

    /// Nest this router into a new Axum router, with the specified path and
    /// using the specified runtime handle.
    ///
    /// This method allows users to specify a runtime handle for the router to
    /// use. This runtime is accessible to all handlers invoked by the router.
    ///
    /// Tasks spawned by the router will be spawned on the provided runtime,
    /// and automatically cancelled when the returned [`axum::Router`] is
    /// dropped.
    #[cfg(feature = "axum")]
    pub fn into_axum_with_handle(
        self,
        path: &str,
        handle: tokio::runtime::Handle,
    ) -> axum::Router<S> {
        axum::Router::new().route(
            path,
            axum::routing::post(crate::axum::IntoAxum::new(self, handle)),
        )
    }
}

impl Router<()> {
    /// Serve the router over a connection. This method returns a
    /// [`ServerShutdown`], which will shut down the server when dropped.
    ///
    /// [`ServerShutdown`]: crate::pubsub::ServerShutdown
    #[cfg(feature = "pubsub")]
    pub async fn serve_pubsub<C: crate::pubsub::Connect>(
        self,
        connect: C,
    ) -> Result<crate::pubsub::ServerShutdown, C::Error> {
        connect.serve(self).await
    }

    /// Create an [`AxumWsCfg`] from this router. This is a convenience method
    /// for `AxumWsCfg::new(self.clone())`.
    ///
    /// [`AxumWsCfg`]: crate::pubsub::AxumWsCfg
    #[cfg(all(feature = "axum", feature = "pubsub"))]
    pub fn to_axum_cfg(&self) -> crate::pubsub::AxumWsCfg {
        crate::pubsub::AxumWsCfg::new(self.clone())
    }

    /// Create an [`axum::Router`] from this router, serving this router via
    /// HTTP `POST` requests at `post_route` and via WebSocket at `ws_route`.
    #[cfg(all(feature = "axum", feature = "pubsub"))]
    pub fn into_axum_with_ws(self, post_route: &str, ws_route: &str) -> axum::Router<()> {
        let cfg = self.to_axum_cfg();

        self.into_axum(post_route)
            .with_state(())
            .route(ws_route, axum::routing::any(crate::pubsub::ajj_websocket))
            .with_state(cfg)
    }

    /// Create an [`axum::Router`] from this router, serving this router via
    /// HTTP `POST` requests at `post_route` and via WebSocket at `ws_route`.
    /// This convenience method allows users to specify a runtime handle for the
    /// router to use. See [`Router::into_axum_with_handle`] for more
    /// information.
    #[cfg(all(feature = "axum", feature = "pubsub"))]
    pub fn into_axum_with_ws_and_handle(
        self,
        post_route: &str,
        ws_route: &str,
        handle: tokio::runtime::Handle,
    ) -> axum::Router<()> {
        let cfg = self.to_axum_cfg();

        self.into_axum_with_handle(post_route, handle)
            .with_state(())
            .route(ws_route, axum::routing::any(crate::pubsub::ajj_websocket))
            .with_state(cfg)
    }

    /// Call a method on the router.
    pub fn handle_request(&self, args: HandlerArgs) -> RouteFuture {
        self.call_with_state(args, ())
    }

    /// Call a batch of methods on the router.
    pub fn handle_request_batch(&self, ctx: HandlerCtx, batch: InboundData) -> BatchFuture {
        self.call_batch_with_state(ctx, batch, ())
    }
}

impl<S> fmt::Debug for Router<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Router").finish_non_exhaustive()
    }
}

impl tower::Service<HandlerArgs> for Router<()> {
    type Response = Option<Box<RawValue>>;
    type Error = Infallible;
    type Future = RouteFuture;

    fn poll_ready(&mut self, _: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, args: HandlerArgs) -> Self::Future {
        self.handle_request(args)
    }
}

impl tower::Service<HandlerArgs> for &Router<()> {
    type Response = Option<Box<RawValue>>;
    type Error = Infallible;
    type Future = RouteFuture;

    fn poll_ready(&mut self, _: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, args: HandlerArgs) -> Self::Future {
        self.handle_request(args)
    }
}

/// The inner state of a [`Router`]. Maps methods to their handlers.
pub(crate) struct RouterInner<S> {
    /// A map from method IDs to their handlers.
    routes: BTreeMap<MethodId, Method<S>>,

    /// The last ID assigned to a method.
    last_id: MethodId,

    /// The handler to call when no method is found.
    fallback: Method<S>,

    // next 2 fields are used for reverse lookup of method names
    /// A map from method names to their IDs.
    name_to_id: BTreeMap<Cow<'static, str>, MethodId>,
    /// A map from method IDs to their names.
    id_to_name: BTreeMap<MethodId, Cow<'static, str>>,
}

impl Default for RouterInner<()> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> fmt::Debug for RouterInner<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RouterInner").finish_non_exhaustive()
    }
}

impl<S> RouterInner<S> {
    /// Create a new, empty router.
    pub(crate) fn new() -> Self {
        Self {
            routes: BTreeMap::new(),

            last_id: Default::default(),

            fallback: Method::Ready(Route::default_fallback()),

            name_to_id: BTreeMap::new(),
            id_to_name: BTreeMap::new(),
        }
    }

    /// Add state to the router, readying methods that require that state.
    ///
    /// Note that the type parameter `S2` is NOT the state you are adding to the
    /// router. It is additional state that must be added AFTER the state `S`.
    pub(crate) fn with_state<S2>(self, state: &S) -> RouterInner<S2>
    where
        S: Clone,
    {
        RouterInner {
            routes: self
                .routes
                .into_iter()
                .map(|(id, method)| (id, method.with_state(state)))
                .collect(),
            fallback: self.fallback.with_state(state),
            last_id: self.last_id,
            name_to_id: self.name_to_id,
            id_to_name: self.id_to_name,
        }
    }

    /// Get the next available ID.
    fn get_id(&mut self) -> MethodId {
        self.last_id += 1;
        self.last_id
    }

    /// Get a method by its name.
    fn method_by_name(&self, name: &str) -> Option<&Method<S>> {
        self.name_to_id.get(name).and_then(|id| self.routes.get(id))
    }

    /// Enroll a method name, returning an ID assignment. Panics if the method
    /// name already exists in the router.
    #[track_caller]
    fn enroll_method_name(
        &mut self,
        method: Cow<'static, str>,
    ) -> Result<MethodId, RegistrationError> {
        if self.name_to_id.contains_key(&method) {
            return Err(RegistrationError::method_already_registered(method));
        }

        let id = self.get_id();
        self.name_to_id.insert(method.clone(), id);
        self.id_to_name.insert(id, method.clone());
        Ok(id)
    }

    /// Enroll a method name, returning an ID assignment.
    ///
    /// # Panics
    ///
    /// Panics if the method
    /// name already exists in the router.
    fn enroll_method(
        &mut self,
        method: Cow<'static, str>,
        handler: Method<S>,
    ) -> Result<MethodId, RegistrationError> {
        self.enroll_method_name(method).inspect(|id| {
            self.routes.insert(*id, handler);
        })
    }

    /// Add a method to the router. This method may be missing state `S`.
    ///
    /// # Panics
    ///
    /// Panics if the method name already exists in the router.
    #[track_caller]
    fn route_erased<E>(mut self, method: impl Into<Cow<'static, str>>, handler: E) -> Self
    where
        E: ErasedIntoRoute<S>,
    {
        let method = method.into();
        let handler = handler.clone_box();

        add_method_inner(&mut self, method, handler);

        fn add_method_inner<S>(
            this: &mut RouterInner<S>,
            method: Cow<'static, str>,
            handler: Box<dyn ErasedIntoRoute<S>>,
        ) {
            panic_on_err!(this.enroll_method(method, Method::Needs(BoxedIntoRoute(handler))));
        }

        self
    }

    /// Add a [`Handler`] to the router.
    ///
    /// # Panics
    ///
    /// Panics if the method name already exists in the router.
    pub(crate) fn route<H, T>(self, method: impl Into<Cow<'static, str>>, handler: H) -> Self
    where
        H: Handler<T, S>,
        T: Send + 'static,
        S: Clone + Send + Sync + 'static,
    {
        self.route_erased(method, MakeErasedHandler::from_handler(handler))
    }

    /// Remove a method from the router.
    ///
    /// If the route does not exist, this method is a no-op.
    pub(crate) fn remove_route(mut self, method: Cow<'static, str>) -> Self {
        if let Some(id) = self.name_to_id.remove(&method) {
            self.id_to_name.remove(&id);
            self.routes.remove(&id);
            self
        } else {
            self
        }
    }

    /// Call a method on the router, with the provided state.
    #[track_caller]
    pub(crate) fn call_with_state(&self, args: HandlerArgs, state: S) -> RouteFuture {
        let method = args.req().method();
        self.method_by_name(method)
            .unwrap_or(&self.fallback)
            .call_with_state(args, state)
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
