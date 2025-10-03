use crate::{pubsub::WriteItem, types::Request, RpcSend, TaskSet};
use serde_json::value::RawValue;
use std::future::Future;
use tokio::{
    sync::mpsc::{self, error::SendError},
    task::JoinHandle,
};
use tokio_util::sync::WaitForCancellationFutureOwned;
use tracing::{enabled, error, Level};

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

    /// The request span.
    pub request_span: tracing::Span,
}

impl TracingInfo {
    /// Create a mock tracing info for testing.
    #[cfg(test)]
    pub fn mock() -> Self {
        use tracing::debug_span;
        Self {
            service: "test",
            request_span: debug_span!("test"),
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

    /// Get a reference to the tracing information for this handler context.
    pub const fn tracing_info(&self) -> &TracingInfo {
        &self.tracing
    }

    /// Get the OpenTelemetry service name for this handler context.
    pub const fn otel_service_name(&self) -> &'static str {
        self.tracing.service
    }

    /// Get a reference to the tracing span for this handler context.
    pub const fn span(&self) -> &tracing::Span {
        &self.tracing.request_span
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
    pub fn new(ctx: HandlerCtx, req: Request) -> Self {
        let this = Self {
            ctx,
            req,
            _seal: (),
        };

        let span = this.span();
        span.record("otel.name", this.otel_span_name());
        span.record("rpc.method", this.req.method());
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
    pub const fn span(&self) -> &tracing::Span {
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
