use std::future::Future;

use tokio::{runtime::Handle, task::JoinHandle};
use tokio_util::{
    sync::{CancellationToken, WaitForCancellationFuture, WaitForCancellationFutureOwned},
    task::{task_tracker::TaskTrackerWaitFuture, TaskTracker},
};

/// This is a wrapper around a [`TaskTracker`] and a [`token`]. It is used to
/// manage a set of tasks, and to token them to shut down when the set is
/// dropped.
#[derive(Debug, Clone, Default)]
pub struct TaskSet {
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
    /// Create a new [`TaskSet`].
    pub fn new() -> Self {
        Self {
            tasks: TaskTracker::new(),
            token: CancellationToken::new(),
            handle: None,
        }
    }

    /// Create a new [`TaskSet`] with a handle.
    pub fn with_handle(handle: Handle) -> Self {
        Self {
            tasks: TaskTracker::new(),
            token: CancellationToken::new(),
            handle: Some(handle),
        }
    }

    /// Get a handle to the runtime that the task set is running on.
    pub fn handle(&self) -> Handle {
        self.handle
            .clone()
            .unwrap_or_else(tokio::runtime::Handle::current)
    }

    /// Close the task set, preventing new tasks from being added.
    ///
    /// See [`TaskTracker::close`]
    pub fn close(&self) {
        self.tasks.close();
    }

    /// Reopen the task set, allowing new tasks to be added.
    ///
    /// See [`TaskTracker::reopen`]
    pub fn reopen(&self) {
        self.tasks.reopen();
    }

    /// Cancel the token, causing all tasks to be cancelled.
    pub fn cancel(&self) {
        self.token.cancel();
    }

    /// Get a future that resolves when the token is fired.
    pub fn cancelled(&self) -> WaitForCancellationFuture<'_> {
        self.token.cancelled()
    }

    /// Get a clone of the cancellation token.
    pub fn cancelled_owned(&self) -> WaitForCancellationFutureOwned {
        self.token.clone().cancelled_owned()
    }

    /// Returns a future that resolves when the token is fired.
    ///
    /// See [`TaskTracker::wait`]
    pub fn wait(&self) -> TaskTrackerWaitFuture<'_> {
        self.tasks.wait()
    }

    /// Convenience function to both fire the token and wait for all tasks to
    /// complete.
    pub fn shutdown(&self) -> TaskTrackerWaitFuture<'_> {
        self.cancel();
        self.wait()
    }

    /// Get a child [`TaskSet`]. This set will be fired when the parent
    /// set is fired, or may be fired independently.
    pub fn child(&self) -> Self {
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
    pub fn spawn<F>(&self, task: F) -> JoinHandle<Option<F::Output>>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.tasks.spawn_on(self.prep_fut(task), &self.handle())
    }

    /// Spawn a blocking future on the provided handle, and add it to the task
    /// set
    pub fn spawn_blocking<F>(&self, task: F) -> JoinHandle<Option<F::Output>>
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
