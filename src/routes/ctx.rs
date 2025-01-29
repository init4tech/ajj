use std::future::Future;

use crate::{types::Request, RpcSend, TaskSet};
use serde_json::value::RawValue;
use tokio::{runtime::Handle, sync::mpsc, task::JoinHandle};
use tracing::error;

/// Errors that can occur when sending notifications.
#[derive(thiserror::Error, Debug)]
pub enum NotifyError {
    /// An error occurred while serializing the notification.
    #[error("failed to serialize notification: {0}")]
    Serde(#[from] serde_json::Error),
    /// The notification channel was closed.
    #[error("notification channel closed")]
    Send(#[from] mpsc::error::SendError<Box<RawValue>>),
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
#[derive(Debug, Clone, Default)]
pub struct HandlerCtx {
    pub(crate) notifications: Option<mpsc::Sender<Box<RawValue>>>,

    /// A task set on which to spawn tasks. This is used to coordinate
    pub(crate) tasks: TaskSet,
}

impl From<TaskSet> for HandlerCtx {
    fn from(tasks: TaskSet) -> Self {
        Self {
            notifications: None,
            tasks,
        }
    }
}

impl From<Handle> for HandlerCtx {
    fn from(handle: Handle) -> Self {
        Self {
            notifications: None,
            tasks: handle.into(),
        }
    }
}

impl HandlerCtx {
    /// Create a new handler context.
    pub(crate) const fn new(
        notifications: Option<mpsc::Sender<Box<RawValue>>>,
        tasks: TaskSet,
    ) -> Self {
        Self {
            notifications,
            tasks,
        }
    }

    /// Get a reference to the notification sender. This is used to
    /// send notifications over pubsub transports.
    pub const fn notifications(&self) -> Option<&mpsc::Sender<Box<RawValue>>> {
        self.notifications.as_ref()
    }

    /// Check if notiifcations can be sent to the client. This will be false
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
            notifications.send(rv).await?;
        }

        Ok(())
    }

    /// Spawn a task on the task set.
    pub fn spawn<F>(&self, f: F) -> JoinHandle<Option<F::Output>>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.tasks.spawn(f)
    }

    /// Spawn a task on the task set with access to this context.
    pub fn spawn_with_ctx<F, Fut>(&self, f: F) -> JoinHandle<Option<Fut::Output>>
    where
        F: FnOnce(HandlerCtx) -> Fut,
        Fut: Future + Send + 'static,
        Fut::Output: Send + 'static,
    {
        self.tasks.spawn(f(self.clone()))
    }

    /// Spawn a task that may block on the task set.
    pub fn spawn_blocking<F, R>(&self, f: F) -> JoinHandle<Option<F::Output>>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.tasks.spawn_blocking(f)
    }

    /// Spawn a task that may block on the task set, with access to this
    /// context.
    pub fn spawn_blocking_with_ctx<F, Fut>(&self, f: F) -> JoinHandle<Option<Fut::Output>>
    where
        F: FnOnce(HandlerCtx) -> Fut,
        Fut: Future + Send + 'static,
        Fut::Output: Send + 'static,
    {
        self.tasks.spawn_blocking(f(self.clone()))
    }
}

/// Arguments passed to a handler.
#[derive(Debug, Clone)]
pub struct HandlerArgs {
    /// The handler context.
    pub(crate) ctx: HandlerCtx,
    /// The JSON-RPC request.
    pub(crate) req: Request,
}

impl HandlerArgs {
    /// Create new handler arguments.
    pub const fn new(ctx: HandlerCtx, req: Request) -> Self {
        Self { ctx, req }
    }

    /// Get a reference to the handler context.
    pub const fn ctx(&self) -> &HandlerCtx {
        &self.ctx
    }

    /// Get a reference to the JSON-RPC request.
    pub const fn req(&self) -> &Request {
        &self.req
    }
}
