use serde_json::value::RawValue;
use tokio::sync::mpsc;

/// A context for handler requests that allow the handler to send notifications
/// from long-running tasks (e.g. subscriptions).
///
/// This is primarily intended to enable subscriptions over pubsub transports
/// to send notifications to clients. It is expected that JSON sent via the
/// notification channel is a valid JSON-RPC object
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
    pub fn new() -> Self {
        Default::default()
    }

    /// Instantiation a new handler context with notifications enabled.
    pub fn with_notifications(notifications: mpsc::Sender<Box<RawValue>>) -> Self {
        Self {
            notifications: Some(notifications),
        }
    }

    /// Get a reference to the notification sender. This is used to
    /// send notifications over pubsub transports.
    pub fn notifications(&self) -> Option<&mpsc::Sender<Box<RawValue>>> {
        self.notifications.as_ref()
    }
}
