use std::future::Future;

use crate::{types::Request, RpcSend, TaskSet};
use serde_json::value::RawValue;
use tokio::{runtime::Handle, sync::mpsc};
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
/// from long-running tasks (e.g. subscriptions).
///
/// This is primarily intended to enable subscriptions over pubsub transports
/// to send notifications to clients. It is expected that JSON sent via the
/// notification channel is a valid JSON-RPC 2.0 object.
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
    pub fn new(notifications: Option<mpsc::Sender<Box<RawValue>>>, tasks: TaskSet) -> Self {
        Self {
            notifications,
            tasks,
        }
    }

    /// Instantiation a new handler context with notifications enabled and a
    /// default [`TaskSet`].
    pub fn notifications_only(notifications: mpsc::Sender<Box<RawValue>>) -> Self {
        Self {
            notifications: Some(notifications),
            tasks: Default::default(),
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
    pub fn spawn<F>(&self, f: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.tasks.spawn(f);
    }

    /// Spawn a task on the task set that may block.
    pub fn spawn_blocking<F, R>(&self, f: F)
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.tasks.spawn_blocking(f);
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
