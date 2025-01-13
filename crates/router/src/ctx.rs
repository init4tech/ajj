use serde_json::value::RawValue;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct HandlerCtx {
    pub(crate) notifications: mpsc::Sender<Box<RawValue>>,
}

impl HandlerCtx {
    /// Instantiate a new handler context.
    pub fn new(notifications: mpsc::Sender<Box<RawValue>>) -> Self {
        Self { notifications }
    }

    /// Get a reference to the notification sender. This is used to
    /// send notifications over pubsub transports.
    pub fn notifications(&self) -> &mpsc::Sender<Box<RawValue>> {
        &self.notifications
    }
}
