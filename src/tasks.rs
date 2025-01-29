use std::future::Future;

use tokio::{runtime::Handle, task::JoinHandle};
use tokio_util::{
    sync::{CancellationToken, WaitForCancellationFuture},
    task::TaskTracker,
};

/// This is a wrapper around a [`TaskTracker`] and a [`CancellationToken`]. It
/// is used to manage a set of tasks, and to token them to shut down when the
/// set is dropped.
///
/// When a [`Handle`] is provided, tasks are spawned on that handle. Otherwise,
/// they are spawned on the current runtime.
#[derive(Debug, Clone, Default)]
pub(crate) struct TaskSet {
    tasks: TaskTracker,
    token: CancellationToken,
    handle: Option<Handle>,
}

impl From<Handle> for TaskSet {
    fn from(handle: Handle) -> Self {
        Self::with_handle(handle)
    }
}

impl TaskSet {
    /// Create a new [`TaskSet`] with a handle.
    pub(crate) fn with_handle(handle: Handle) -> Self {
        Self {
            tasks: TaskTracker::new(),
            token: CancellationToken::new(),
            handle: Some(handle),
        }
    }

    /// Get a handle to the runtime that the task set is running on.
    ///
    /// ## Panics
    ///
    /// This will panic if called outside the context of a Tokio runtime.
    pub(crate) fn handle(&self) -> Handle {
        self.handle
            .clone()
            .unwrap_or_else(tokio::runtime::Handle::current)
    }

    /// Cancel the token, causing all tasks to be cancelled.
    pub(crate) fn cancel(&self) {
        self.token.cancel();
    }

    pub(crate) async fn shutdown(&self) {
        self.cancel();
        self.tasks.wait().await
    }

    /// Get a future that resolves when the token is fired.
    pub(crate) fn cancelled(&self) -> WaitForCancellationFuture<'_> {
        self.token.cancelled()
    }

    /// Get a child [`TaskSet`]. This set will be fired when the parent
    /// set is fired, or may be fired independently.
    pub(crate) fn child(&self) -> Self {
        Self {
            tasks: TaskTracker::new(),
            token: self.token.child_token(),
            handle: self.handle.clone(),
        }
    }

    /// Prepare a future to be added to the task set, by wrapping it with a
    /// cancellation token.
    fn prep_fut<F>(&self, task: F) -> impl Future<Output = Option<F::Output>> + Send + 'static
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let token = self.token.clone();
        async move {
            tokio::select! {
                _ = token.cancelled() => None,
                result = task => Some(result),
            }
        }
    }

    /// Spawn a future on the provided handle, and add it to the task set.
    ///
    /// ## Panics
    ///
    /// This will panic if called outside the context of a Tokio runtime when
    /// `self.handle` is `None`.
    pub(crate) fn spawn<F>(&self, task: F) -> JoinHandle<Option<F::Output>>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.tasks.spawn_on(self.prep_fut(task), &self.handle())
    }

    /// Spawn a blocking future on the provided handle, and add it to the task
    /// set.
    ///
    /// ## Panics
    ///
    /// This will panic if called outside the context of a Tokio runtime when
    /// `self.handle` is `None`.
    pub(crate) fn spawn_blocking<F>(&self, task: F) -> JoinHandle<Option<F::Output>>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let h = self.handle();
        let task = self.prep_fut(task);
        self.tasks
            .spawn_blocking_on(move || h.block_on(task), &self.handle())
    }
}
