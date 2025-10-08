use crate::{pubsub::WriteItem, types::Request, Router, RpcSend, TaskSet};
use ::tracing::info_span;
use opentelemetry::trace::TraceContextExt;
use serde_json::value::RawValue;
use std::{future::Future, sync::OnceLock};
use tokio::{
    sync::mpsc::{self, error::SendError},
    task::JoinHandle,
};
use tokio_util::sync::WaitForCancellationFutureOwned;
use tracing::{enabled, error, Level};
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// Errors that can occur when sending notifications.
#[derive(thiserror::Error, Debug)]
pub enum NotifyError {
    /// An error occurred while serializing the notification.
    #[error("failed to serialize notification: {0}")]
    Serde(#[from] serde_json::Error),
    /// The notification channel was closed.
    #[error("notification channel closed")]
    Send(#[from] SendError<Box<RawValue>>),
}

impl From<SendError<WriteItem>> for NotifyError {
    fn from(value: SendError<WriteItem>) -> Self {
        SendError(value.0.json).into()
    }
}

/// Tracing information for OpenTelemetry. This struct is used to store
/// information about the current request that can be used for tracing.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TracingInfo {
    /// The OpenTelemetry service name.
    pub service: &'static str,

    /// The open telemetry Context,
    pub context: Option<opentelemetry::context::Context>,

    /// The tracing span for this request.
    span: OnceLock<tracing::Span>,
}

impl TracingInfo {
    /// Create a new tracing info with the given service name and no context.
    #[allow(dead_code)] // used in some features
    pub const fn new(service: &'static str) -> Self {
        Self {
            service,
            context: None,
            span: OnceLock::new(),
        }
    }

    /// Create a new tracing info with the given service name and context.
    pub const fn new_with_context(
        service: &'static str,
        context: opentelemetry::context::Context,
    ) -> Self {
        Self {
            service,
            context: Some(context),
            span: OnceLock::new(),
        }
    }

    fn make_span<S>(
        &self,
        router: &Router<S>,
        with_notifications: bool,
        parent: Option<&tracing::Span>,
    ) -> tracing::Span
    where
        S: Clone + Send + Sync + 'static,
    {
        let span = info_span!(
            parent: parent.and_then(|p| p.id()),
            "AjjRequest",
            "otel.kind" = "server",
            "rpc.system" = "jsonrpc",
            "rpc.jsonrpc.version" = "2.0",
            "rpc.service" = router.service_name(),
            notifications_enabled = with_notifications,
            "trace_id" = ::tracing::field::Empty,
            "otel.name" = ::tracing::field::Empty,
            "otel.status_code" = ::tracing::field::Empty,
            "rpc.jsonrpc.request_id" = ::tracing::field::Empty,
            "rpc.jsonrpc.error_code" = ::tracing::field::Empty,
            "rpc.jsonrpc.error_message" = ::tracing::field::Empty,
            "rpc.method" = ::tracing::field::Empty,
            params = ::tracing::field::Empty,
        );
        if let Some(context) = &self.context {
            let _ = span.set_parent(context.clone());

            span.record(
                "trace_id",
                context.span().span_context().trace_id().to_string(),
            );
        }
        span
    }

    /// Create a request span for a handler invocation.
    fn init_request_span<S>(
        &self,
        router: &Router<S>,
        with_notifications: bool,
        parent: Option<&tracing::Span>,
    ) -> &tracing::Span
    where
        S: Clone + Send + Sync + 'static,
    {
        // This span is populated with as much detail as possible, and then
        // given to the Request. It will be populated with request-specific
        // details (e.g. method) during request setup.
        self.span
            .get_or_init(|| self.make_span(router, with_notifications, parent))
    }

    /// Create a child tracing info for a new handler context.
    pub fn child<S: Clone + Send + Sync + 'static>(
        &self,
        router: &Router<S>,
        with_notifications: bool,
        parent: Option<&tracing::Span>,
    ) -> Self {
        let span = self.make_span(router, with_notifications, parent);
        Self {
            service: self.service,
            context: self.context.clone(),
            span: OnceLock::from(span),
        }
    }

    /// Get a reference to the tracing span for this request.
    ///
    /// ## Panics
    ///
    /// Panics if the span has not been initialized via
    /// [`Self::init_request_span`].
    #[track_caller]
    fn request_span(&self) -> &tracing::Span {
        self.span.get().expect("span not initialized")
    }

    /// Create a mock tracing info for testing.
    #[cfg(test)]
    pub fn mock() -> Self {
        Self {
            service: "test",
            context: None,
            span: OnceLock::from(info_span!("")),
        }
    }
}

/// A context for handler requests that allow the handler to send notifications
/// and spawn long-running tasks (e.g. subscriptions).
///
/// The handler is used for two things:
/// - Spawning long-running tasks (e.g. subscriptions) via
///   [`HandlerCtx::spawn`] or [`HandlerCtx::spawn_blocking`].
/// - Sending notifications to pubsub clients via [`HandlerCtx::notify`].
///   Notifcations SHOULD be valid JSON-RPC objects, but this is
///   not enforced by the type system.
#[derive(Debug, Clone)]
pub struct HandlerCtx {
    pub(crate) notifications: Option<mpsc::Sender<WriteItem>>,

    /// A task set on which to spawn tasks. This is used to coordinate
    pub(crate) tasks: TaskSet,

    /// Tracing information for OpenTelemetry.
    pub(crate) tracing: TracingInfo,
}

impl HandlerCtx {
    /// Create a new handler context.
    pub(crate) const fn new(
        notifications: Option<mpsc::Sender<WriteItem>>,
        tasks: TaskSet,
        tracing: TracingInfo,
    ) -> Self {
        Self {
            notifications,
            tasks,
            tracing,
        }
    }

    /// Create a mock handler context for testing.
    #[cfg(test)]
    pub fn mock() -> Self {
        Self {
            notifications: None,
            tasks: TaskSet::default(),
            tracing: TracingInfo::mock(),
        }
    }

    /// Create a child handler context for a new handler invocation.
    ///
    /// This is used when handling batch requests, to give each handler
    /// its own context.
    pub fn child_ctx<S: Clone + Send + Sync + 'static>(
        &self,
        router: &Router<S>,
        parent: Option<&tracing::Span>,
    ) -> Self {
        Self {
            notifications: self.notifications.clone(),
            tasks: self.tasks.clone(),
            tracing: self
                .tracing
                .child(router, self.notifications_enabled(), parent),
        }
    }

    /// Get a reference to the tracing information for this handler context.
    pub const fn tracing_info(&self) -> &TracingInfo {
        &self.tracing
    }

    /// Get the OpenTelemetry service name for this handler context.
    pub const fn otel_service_name(&self) -> &'static str {
        self.tracing.service
    }

    /// Get a reference to the tracing span for this handler context.
    #[track_caller]
    pub fn span(&self) -> &tracing::Span {
        self.tracing.request_span()
    }

    /// Set the tracing information for this handler context.
    pub fn set_tracing_info(&mut self, tracing: TracingInfo) {
        self.tracing = tracing;
    }

    /// Check if notifications can be sent to the client. This will be false
    /// when either the transport does not support notifications, or the
    /// notification channel has been closed (due the the client going away).
    pub fn notifications_enabled(&self) -> bool {
        self.notifications
            .as_ref()
            .map(|tx| !tx.is_closed())
            .unwrap_or_default()
    }

    /// Create a request span for a handler invocation.
    pub fn init_request_span<S>(
        &self,
        router: &Router<S>,
        parent: Option<&tracing::Span>,
    ) -> &tracing::Span
    where
        S: Clone + Send + Sync + 'static,
    {
        self.tracing_info()
            .init_request_span(router, self.notifications_enabled(), parent)
    }

    /// Notify a client of an event.
    pub async fn notify<T: RpcSend>(&self, t: &T) -> Result<(), NotifyError> {
        if let Some(notifications) = self.notifications.as_ref() {
            let rv = serde_json::value::to_raw_value(t)?;
            notifications
                .send(WriteItem {
                    span: self.span().clone(),
                    json: rv,
                })
                .await?;
        }

        Ok(())
    }

    /// Spawn a task on the task set. This task will be cancelled if the
    /// client disconnects. This is useful for long-running server tasks.
    ///
    /// The resulting [`JoinHandle`] will contain [`None`] if the task was
    /// cancelled, and `Some` otherwise.
    pub fn spawn<F>(&self, f: F) -> JoinHandle<Option<F::Output>>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.tasks.spawn_cancellable(f)
    }

    /// Spawn a task on the task set with access to this context. This
    /// task will be cancelled if the client disconnects. This is useful
    /// for long-running tasks like subscriptions.
    ///
    /// The resulting [`JoinHandle`] will contain [`None`] if the task was
    /// cancelled, and `Some` otherwise.
    pub fn spawn_with_ctx<F, Fut>(&self, f: F) -> JoinHandle<Option<Fut::Output>>
    where
        F: FnOnce(HandlerCtx) -> Fut,
        Fut: Future + Send + 'static,
        Fut::Output: Send + 'static,
    {
        self.tasks.spawn_cancellable(f(self.clone()))
    }

    /// Spawn a task that may block on the task set. This task may block, and
    /// will be cancelled if the client disconnects. This is useful for
    /// running expensive tasks that require blocking IO (e.g. database
    /// queries).
    ///
    /// The resulting [`JoinHandle`] will contain [`None`] if the task was
    /// cancelled, and `Some` otherwise.
    pub fn spawn_blocking<F>(&self, f: F) -> JoinHandle<Option<F::Output>>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.tasks.spawn_blocking_cancellable(f)
    }

    /// Spawn a task that may block on the task set, with access to this
    /// context. This task may block, and will be cancelled if the client
    /// disconnects. This is useful for running expensive tasks that require
    /// blocking IO (e.g. database queries).
    ///
    /// The resulting [`JoinHandle`] will contain [`None`] if the task was
    /// cancelled, and `Some` otherwise.
    pub fn spawn_blocking_with_ctx<F, Fut>(&self, f: F) -> JoinHandle<Option<Fut::Output>>
    where
        F: FnOnce(HandlerCtx) -> Fut,
        Fut: Future + Send + 'static,
        Fut::Output: Send + 'static,
    {
        self.tasks.spawn_blocking_cancellable(f(self.clone()))
    }

    /// Spawn a task on this task set. Unlike [`Self::spawn`], this task will
    /// NOT be cancelled if the client disconnects. Instead, it
    /// is given a future that resolves when client disconnects. This is useful
    /// for tasks that need to clean up resources before completing.
    pub fn spawn_graceful<F, Fut>(&self, f: F) -> JoinHandle<Fut::Output>
    where
        F: FnOnce(WaitForCancellationFutureOwned) -> Fut + Send + 'static,
        Fut: Future + Send + 'static,
        Fut::Output: Send + 'static,
    {
        self.tasks.spawn_graceful(f)
    }

    /// Spawn a task on this task set with access to this context. Unlike
    /// [`Self::spawn`], this task will NOT be cancelled if the client
    /// disconnects. Instead, it is given a future that resolves when client
    /// disconnects. This is useful for tasks that need to clean up resources
    /// before completing.
    pub fn spawn_graceful_with_ctx<F, Fut>(&self, f: F) -> JoinHandle<Fut::Output>
    where
        F: FnOnce(HandlerCtx, WaitForCancellationFutureOwned) -> Fut + Send + 'static,
        Fut: Future + Send + 'static,
        Fut::Output: Send + 'static,
    {
        let ctx = self.clone();
        self.tasks.spawn_graceful(move |token| f(ctx, token))
    }

    /// Spawn a blocking task on this task set. Unlike [`Self::spawn_blocking`],
    /// this task will NOT be cancelled if the client disconnects. Instead, it
    /// is given a future that resolves when client disconnects. This is useful
    /// for tasks that need to clean up resources before completing.
    pub fn spawn_blocking_graceful<F, Fut>(&self, f: F) -> JoinHandle<Fut::Output>
    where
        F: FnOnce(WaitForCancellationFutureOwned) -> Fut + Send + 'static,
        Fut: Future + Send + 'static,
        Fut::Output: Send + 'static,
    {
        self.tasks.spawn_blocking_graceful(f)
    }

    /// Spawn a blocking task on this task set with access to this context.
    /// Unlike [`Self::spawn_blocking`], this task will NOT be cancelled if the
    /// client disconnects. Instead, it is given a future that resolves when
    /// the client disconnects. This is useful for tasks that need to clean up
    /// resources before completing.
    pub fn spawn_blocking_graceful_with_ctx<F, Fut>(&self, f: F) -> JoinHandle<Fut::Output>
    where
        F: FnOnce(HandlerCtx, WaitForCancellationFutureOwned) -> Fut + Send + 'static,
        Fut: Future + Send + 'static,
        Fut::Output: Send + 'static,
    {
        let ctx = self.clone();
        self.tasks
            .spawn_blocking_graceful(move |token| f(ctx, token))
    }
}

/// Arguments passed to a handler.
#[derive(Debug, Clone)]
pub struct HandlerArgs {
    /// The handler context.
    ctx: HandlerCtx,
    /// The JSON-RPC request.
    req: Request,

    /// prevent instantation outside of this module
    _seal: (),
}

impl HandlerArgs {
    /// Create new handler arguments.
    ///
    /// ## Panics
    ///
    /// If the ctx tracing span has not been initialized via
    /// [`HandlerCtx::init_request_span`].
    #[track_caller]
    pub fn new(ctx: HandlerCtx, req: Request) -> Self {
        let this = Self {
            ctx,
            req,
            _seal: (),
        };

        let span = this.span();
        span.record("otel.name", this.otel_span_name());
        span.record("rpc.method", this.req.method());
        span.record("rpc.jsonrpc.request_id", this.req.id());
        if enabled!(Level::TRACE) {
            span.record("params", this.req.params());
        }

        this
    }

    /// Decompose the handler arguments into its parts.
    pub fn into_parts(self) -> (HandlerCtx, Request) {
        (self.ctx, self.req)
    }

    /// Get a reference to the handler context.
    pub const fn ctx(&self) -> &HandlerCtx {
        &self.ctx
    }

    /// Get a reference to the tracing span for this handler invocation.
    ///
    /// ## Panics
    ///
    /// If the span has not been initialized via
    /// [`HandlerCtx::init_request_span`].
    #[track_caller]
    pub fn span(&self) -> &tracing::Span {
        self.ctx.span()
    }

    /// Get a reference to the JSON-RPC request.
    pub const fn req(&self) -> &Request {
        &self.req
    }

    /// Get the ID of the JSON-RPC request, if any.
    pub fn id_owned(&self) -> Option<Box<RawValue>> {
        self.req.id_owned()
    }

    /// Get the OpenTelemetry span name for this handler invocation.
    pub fn otel_span_name(&self) -> String {
        format!("{}/{}", self.ctx.otel_service_name(), self.req.method())
    }
}
