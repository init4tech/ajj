use serde_json::value::RawValue;
use tokio::sync::mpsc;
use tracing::error;

use crate::RpcSend;

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
}

impl From<mpsc::Sender<Box<RawValue>>> for HandlerCtx {
    fn from(notifications: mpsc::Sender<Box<RawValue>>) -> Self {
        Self {
            notifications: Some(notifications),
        }
    }
}

impl HandlerCtx {
    /// Instantiate a new handler context.
    pub const fn new() -> Self {
        Self {
            notifications: None,
        }
    }

    /// Instantiation a new handler context with notifications enabled.
    pub const fn with_notifications(notifications: mpsc::Sender<Box<RawValue>>) -> Self {
        Self {
            notifications: Some(notifications),
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
}

/// Arguments passed to a handler.
#[derive(Debug, Clone)]
pub struct HandlerArgs {
    /// The handler context.
    pub ctx: HandlerCtx,
    /// The JSON-RPC request.
    pub req: crate::types::Request,
}
